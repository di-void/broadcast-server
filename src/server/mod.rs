use async_tungstenite::accept_async;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::{Message, protocol::CloseFrame};
use ctrlc;
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
    id: u8,
    sender: Sender<Message>,
}

#[derive(Debug)]
enum ChannelMessage {
    Text(u8, Message),
    Close(u8),
    ShutDown,
}

pub async fn start() -> io::Result<()> {
    let registry: HashMap<u8, Client> = HashMap::new();
    let tasks_table: HashMap<u8, Task<()>> = HashMap::new();
    let registry_lock = Arc::new(Mutex::new(registry));
    let tasks_table_lock = Arc::new(Mutex::new(tasks_table));
    let (tx, rx) = channel::unbounded::<ChannelMessage>();
    let (sd_tx, shutdown) = channel::bounded(1);

    let reg_lock = Arc::clone(&registry_lock);
    let tasks_lock = Arc::clone(&tasks_table_lock);

    let ch_sender = tx.clone();
    ctrlc::set_handler(move || {
        ch_sender.send_blocking(ChannelMessage::ShutDown).unwrap();
    })
    .expect("Couldn't setup ctrlc handler");

    let broker_task = smol::spawn(async move {
        // broker
        println!("Broker is active!");

        loop {
            let msg = rx.recv().await.unwrap();

            match msg {
                ChannelMessage::Text(cid, msg) => {
                    let mut reg_lock_guard = reg_lock.lock().await;
                    println!("Broker has acquired client registry lock!");

                    for (_, client) in reg_lock_guard.iter_mut() {
                        if client.id != cid {
                            println!("Sending to client {}", client.id);
                            client.sender.send(msg.clone()).await.unwrap();
                            println!("Sent!");
                        }
                    }
                }
                ChannelMessage::Close(cid) => {
                    let mut tasks_table_guard = tasks_lock.lock().await;
                    println!("Broker has acquired tasks table lock!");

                    let task = tasks_table_guard.remove(&cid).unwrap();
                    task.cancel().await; // could deadlock?
                    println!("Main handler task for client {cid} has been cancelled");
                }
                ChannelMessage::ShutDown => {
                    println!("Shutting down!!");
                    let mut reg_lock_guard = reg_lock.lock().await;
                    let mut tasks_table_guard = tasks_lock.lock().await;

                    for (_, client) in reg_lock_guard.iter_mut() {
                        client
                            .sender
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Away,
                                reason: "Server is shutting down".into(),
                            })))
                            .await
                            .unwrap();
                    }

                    let tkeys = tasks_table_guard
                        .keys()
                        .map(|k| k.to_owned())
                        .collect::<Vec<_>>();
                    for k in tkeys {
                        let task = tasks_table_guard.remove(&k).unwrap();
                        task.cancel().await;
                    }

                    reg_lock_guard.clear();
                    tasks_table_guard.clear();

                    sd_tx.send(()).await.unwrap();
                    break;
                }
            }
        }

        println!("Broker is cancelling...");
    });

    let main_task = smol::spawn(async move {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 8080)))
            .await
            .unwrap();
        let mut incoming = listener.incoming();
        println!("Server is listening on port 8080!");
        let mut cc: u8 = 0; // client count; max of 255

        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            println!("new connection!");

            println!("Starting websocket session!");
            let ws_stream = accept_async(stream).await.unwrap();
            let (mut ws_sender, mut ws_receiver) = ws_stream.split();
            let cid = {
                cc += 1;
                cc
            };
            let (b_tx, b_rx) = channel::bounded(2);
            let new_connection = Client {
                id: cid,
                sender: b_tx,
            };

            let mut registry_writer = registry_lock.lock().await;
            registry_writer.insert(cid, new_connection);
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
            let main_handler = smol::spawn(async move {
                println!("Client {cid} has spawned into action");
                let tx = sender;

                loop {
                    // read a message from ws stream
                    if let Some(res) = ws_receiver.next().await {
                        match res {
                            Ok(msg) => {
                                if msg.is_text() {
                                    let channel_msg = ChannelMessage::Text(cid, msg);
                                    tx.send_blocking(channel_msg).unwrap(); // deadlock?
                                } else if msg.is_close() {
                                    let mut w = reg_lock2.lock().await;
                                    let _ = w.remove(&cid); // remove client from registry
                                    bcast_handler_task.cancel().await; // clean up
                                    println!(
                                        "Broadcast handler task for client {cid} has been cancelled"
                                    );
                                    tx.send_blocking(ChannelMessage::Close(cid)).unwrap();
                                    break;
                                }
                            }
                            Err(e) => {
                                println!("Something went wrong: {:?}", e);

                                let mut w = reg_lock2.lock().await;
                                let _ = w.remove(&cid);
                                bcast_handler_task.cancel().await; // clean up
                                println!(
                                    "Broadcast handler task for client {cid} has been cancelled"
                                );
                                tx.send_blocking(ChannelMessage::Close(cid)).unwrap();
                                break;
                            }
                        }
                    }
                }
            });

            let mut tasks_table_guard = tasks_table_lock.lock().await;
            tasks_table_guard.insert(cid, main_handler);
        }
    });

    let _ = shutdown.recv().await.unwrap();
    main_task.cancel().await;
    broker_task.cancel().await;

    Ok(())
}
