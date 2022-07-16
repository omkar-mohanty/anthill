pub mod client;
pub mod commands;
pub mod config;
pub mod eventloop;
pub mod events;

use async_std::task::spawn;
use client::Client;
use config::Config;
use events::Event;
use futures::channel::mpsc;

pub async fn with_config(
    config: Config,
) -> Result<(Client, mpsc::Receiver<Event>), Box<dyn std::error::Error + Send>> {
    let (command_sender, command_receiver) = mpsc_tuple(&config);

    let (event_sender, event_receiver) = mpsc_tuple(&config);

    let eventloop = eventloop::EventLoop::new(event_sender, command_receiver)
        .await
        .expect("Could not create event loop");

    spawn(eventloop.run());

    Ok((Client { command_sender }, event_receiver))
}

pub async fn new() -> Result<(Client, mpsc::Receiver<Event>), Box<dyn std::error::Error + Send>> {
    let (command_sender, command_receiver) = mpsc::channel(1);

    let (event_sender, event_receiver) = mpsc::channel(1);

    let eventloop = eventloop::EventLoop::new(event_sender, command_receiver)
        .await
        .expect("Could not create event loop");

    spawn(eventloop.run());

    Ok((Client { command_sender }, event_receiver))
}

fn mpsc_tuple<T>(config: &Config) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
    if let Some(buff_size) = config.buffer {
        mpsc::channel(buff_size)
    } else {
        mpsc::channel(1)
    }
}
