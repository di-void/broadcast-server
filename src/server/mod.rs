use async_tungstenite::accept_async;
use async_tungstenite::tungstenite::Message;
use rand::prelude::*;
use smol::{
    self, Task,
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
    sender: Sender<Message>,
}

#[derive(Debug)]
enum ChannelMessage {
    Text(u16, Message),
    Close(u16),
}

pub async fn start() -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 8080))).await?;
    let mut incoming = listener.incoming();
    println!("Server is listening on port 8080!");

    let mut rng = rand::rng();
    let ids = (1..=100).collect::<Vec<u16>>(); // client id pool

    let registry: HashMap<u16, Client> = HashMap::new();
    let tasks_table: HashMap<u16, Task<()>> = HashMap::new();
    let registry_lock = Arc::new(Mutex::new(registry));
    let tasks_table_lock = Arc::new(Mutex::new(tasks_table));
    let (tx, rx) = channel::unbounded::<ChannelMessage>();

    let reg_lock = Arc::clone(&registry_lock);
    let tasks_lock = Arc::clone(&tasks_table_lock);
    smol::spawn(async move {
        // broker
        println!("Broker is active!");

        loop {
            let msg = rx.recv().await.unwrap();

            match msg {
                ChannelMessage::Text(client_id, msg) => {
                    let mut reg_lock_guard = reg_lock.lock().await;
                    println!("Broker has acquired client registry lock!");

                    for (_, client) in reg_lock_guard.iter_mut() {
                        if client.id != client_id {
                            println!("Sending to client {}", client.id);
                            client.sender.send(msg.clone()).await.unwrap();
                            println!("Sent!");
                        }
                    }
                }
                ChannelMessage::Close(client_id) => {
                    let mut tasks_table_guard = tasks_lock.lock().await;
                    println!("Broker has acquired tasks table lock!");

                    let task = tasks_table_guard.remove(&client_id).unwrap();
                    task.cancel().await; // could deadlock?
                    println!("Main task for client {client_id} has been cancelled");
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
            sender: b_tx,
        };

        let mut registry_writer = registry_lock.lock().await;
        registry_writer.insert(client_id, new_connection);
        drop(registry_writer); // release write lock

        let sender = tx.clone();

        let bcast_handler_task = smol::spawn(async move {
            // respond to broadcast
            loop {
                let msg = b_rx.recv().await.unwrap();
                ws_sender.send(msg).await.unwrap();
            }
        });

        let reg_lock2 = Arc::clone(&registry_lock);
        let main_task = smol::spawn(async move {
            println!("Client {client_id} has spawned into action");

            let c_id = client_id;
            let tx = sender;

            loop {
                // read a message from ws stream
                if let Some(res) = ws_receiver.next().await {
                    match res {
                        Ok(msg) => {
                            if msg.is_text() {
                                let channel_msg = ChannelMessage::Text(c_id.clone(), msg);
                                tx.send_blocking(channel_msg).unwrap(); // deadlock?
                            } else if msg.is_close() {
                                let mut w = reg_lock2.lock().await;
                                let _ = w.remove(&c_id); // remove client from registry
                                drop(w); // releast mutex lock

                                bcast_handler_task.cancel().await; // clean up
                                println!(
                                    "Broadcast handler task for client {c_id} has been cancelled"
                                );
                                tx.send_blocking(ChannelMessage::Close(c_id)).unwrap();
                                break;
                            }
                        }
                        Err(e) => {
                            println!("Something went wrong: {:?}", e);

                            let mut w = reg_lock2.lock().await;
                            let _ = w.remove(&c_id); // remove client from registry
                            drop(w); // releast mutex lock

                            bcast_handler_task.cancel().await; // clean up
                            println!("Broadcast handler task for client {c_id} has been cancelled");
                            tx.send_blocking(ChannelMessage::Close(c_id)).unwrap();
                            break;
                        }
                    }
                }
            }
        });

        let mut tasks_table_guard = tasks_table_lock.lock().await;
        tasks_table_guard.insert(client_id, main_task);
    }

    Ok(())
}
