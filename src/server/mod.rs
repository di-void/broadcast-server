use async_tungstenite::tungstenite::Message;
use async_tungstenite::{WebSocketStream, accept_async};
use rand::prelude::*;
use smol::{
    self, Timer, channel,
    lock::RwLock,
    net::{TcpListener, TcpStream},
    stream::StreamExt,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{io, net::SocketAddr};

// mod buf;

#[derive(Debug)]
struct Client {
    id: u16, // maybe redundant?
    is_active: bool,
    is_connected: bool,
    ws_stream: WebSocketStream<TcpStream>,
}

#[derive(Debug)]
struct ChannelMessage {
    client_id: u16,
    message: Message,
}
// - to avoid runaway memory, message buffer should be bounded // 2kb?
// - evict disconnected clients from state

pub async fn start() -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 8080))).await?;
    let mut incoming = listener.incoming();
    println!("Server is listening on port 8080!");

    let mut rng = rand::rng();
    let ids = (1..=100).collect::<Vec<u16>>(); // client id pool

    let registry: HashMap<u16, Client> = HashMap::new();
    let registry_lock = Arc::new(RwLock::new(registry));
    let (tx, rx) = channel::unbounded::<ChannelMessage>();

    let reg_lock = Arc::clone(&registry_lock);
    smol::spawn(async move {
        // broker
        println!("Broker is active!");

        loop {
            // let msg = rx.recv_async().await.unwrap();
            let msg = rx.recv().await.unwrap();
            println!("Broker received msg: {:#?}", &msg);
            // consume message from mpsc channel

            let mut writer = reg_lock.write_arc().await;
            println!("Broker has acquired write-lock!");

            for (_, client) in writer.iter_mut() {
                // except client with received client id
                if client.id != msg.client_id {
                    println!("Sending to client {}", client.id);
                    client.ws_stream.send(msg.message.clone()).await.unwrap();
                }
            }
        }
    })
    .detach();

    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        println!("new connection!");

        println!("Starting websocket session!");
        let ws_stream = accept_async(stream).await.unwrap();
        let client_id = ids.choose(&mut rng).unwrap().to_owned();

        let new_connection = Client {
            id: client_id,
            is_active: true,
            is_connected: true,
            ws_stream,
        };

        let mut registry_writer = registry_lock.write_arc().await;
        registry_writer.insert(client_id, new_connection);
        drop(registry_writer); // release write lock

        let reg_lock = Arc::clone(&registry_lock);
        let sender = tx.clone();
        smol::spawn(async move {
            println!("Client {client_id} has spawned into action");

            let c_id = client_id;
            let tx = sender;
            // could possibly block a bit here to first
            // fill a buffer with messages so we don't have
            // to acquire a lock for some time

            loop {
                let mut writer = reg_lock.write_arc().await;
                println!("Client {client_id} has acquired write-lock!");

                // read a message from ws stream
                let client = writer.get_mut(&c_id).unwrap();
                if let Some(msg) = client.ws_stream.next().await {
                    println!("Client {} got new message: {:#?}", c_id, &msg);
                    let msg = msg.unwrap();

                    let channel_msg: ChannelMessage = ChannelMessage {
                        client_id: c_id.clone(),
                        message: msg,
                    };

                    println!(
                        "Client {} sending message to broker: {:#?}",
                        c_id, &channel_msg
                    );

                    tx.send_blocking(channel_msg).unwrap(); // deadlock?
                    drop(writer); // release write-lock

                    // allow other writers to progress
                    Timer::after(Duration::from_millis(800)).await;

                    Some(())
                } else {
                    // stream has finished
                    break;
                };
            }
        })
        .detach();
    }

    Ok(())
}
