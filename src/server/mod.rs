use async_tungstenite::accept_async;
use async_tungstenite::tungstenite::Message;
use rand::prelude::*;
use smol::{
    self,
    channel::{self, Sender},
    lock::Mutex,
    net::TcpListener,
    stream::StreamExt,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::{io, net::SocketAddr};

#[derive(Debug)]
struct Client {
    id: u16,
    is_active: bool,
    is_connected: bool,
    sender: Sender<Message>,
}

#[derive(Debug)]
struct ChannelMessage {
    client_id: u16,
    message: Message,
}
// - evict disconnected clients from state

pub async fn start() -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 8080))).await?;
    let mut incoming = listener.incoming();
    println!("Server is listening on port 8080!");

    let mut rng = rand::rng();
    let ids = (1..=100).collect::<Vec<u16>>(); // client id pool

    let registry: HashMap<u16, Client> = HashMap::new();
    let registry_lock = Arc::new(Mutex::new(registry));
    let (tx, rx) = channel::unbounded::<ChannelMessage>();

    let reg_lock = Arc::clone(&registry_lock);
    smol::spawn(async move {
        // broker
        println!("Broker is active!");

        loop {
            let msg = rx.recv().await.unwrap();
            let mut guard = reg_lock.lock().await;
            println!("Broker has acquired lock!");

            for (_, client) in guard.iter_mut() {
                if client.id != msg.client_id {
                    println!("Sending to client {}", client.id);
                    client.sender.send(msg.message.clone()).await.unwrap();
                    println!("Sent!");
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
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let client_id = ids.choose(&mut rng).unwrap().to_owned();
        let (b_tx, b_rx) = channel::bounded(2);

        let new_connection = Client {
            id: client_id,
            is_active: true,
            is_connected: true,
            sender: b_tx,
        };

        let mut registry_writer = registry_lock.lock().await;
        registry_writer.insert(client_id, new_connection);
        drop(registry_writer); // release write lock

        let sender = tx.clone();

        smol::spawn(async move {
            // respond to broadcast
            loop {
                let msg = b_rx.recv().await.unwrap();
                ws_sender.send(msg).await.unwrap();
            }
        })
        .detach();

        smol::spawn(async move {
            println!("Client {client_id} has spawned into action");

            let c_id = client_id;
            let tx = sender;
            // could possibly block a bit here to first
            // fill a buffer with messages so we don't have
            // to acquire a lock for some time

            loop {
                // read a message from ws stream
                if let Some(msg) = ws_receiver.next().await {
                    let msg = msg.unwrap();

                    let channel_msg: ChannelMessage = ChannelMessage {
                        client_id: c_id.clone(),
                        message: msg,
                    };

                    tx.send_blocking(channel_msg).unwrap(); // deadlock?
                } else {
                    break;
                }
            }
        })
        .detach();
    }

    Ok(())
}
