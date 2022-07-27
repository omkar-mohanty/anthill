

# Imhoteph
A peer to peer chat application using libp2p Swarm and Gossipsub protocols. 
# Motivation
Almost all major instant messaging apps are made by big tech who have a less than stellar track record of respecting user privacy. Imhoteph aims to be reliable, scalable and at the same time respect the privacy of the user. 
# Tech used
- Rust nightly
- [Libp2p](https://github.com/libp2p/rust-libp2p)
- [Clap](https://docs.rs/clap/latest/clap/)
- [Tokio](https://tokio.rs/)
# Usage 
`imhoteph-cli [SUBCOMMAND]`
## Sub commands
- `dial --id [PEER ID] --topic [TOPIC NAME]`
  Dial the given Peer ID and subscribe to the topic 
- `test --id [PEER ID] --addr [PEER MULTIADDR]`
  Test dial a the given peer id with the given multiaddr.
# Working 
There are several independent working parts of Imhoteph.
1. `network` Manages network events, establishes connections and publishes messages to the network.
3. `cli` Handles commands given by the user 
### Network Initialization
On startup two threads are spawned one for displaying the messages and notifying the user about network events the other thread manages the network `Eventloop` struct.
```rust 
// Run the network eventloop
pub async fn run(mut self) {
        loop {
            futures::select! {
                event = self.swarm.select_next_some() => {
                        self.handle_event(event).await;
                }

                command = self.command_receiver.select_next_some() => {
                        self.handle_command(command).await;
                }
            }
        }
    }
```
### Message Passing
The `Eventloop` struct has `event_sender` field of type `mpsc::channel` which is used to pass messages to other the corresponding `event_listner` in the Client.
###  Network Events
There are a number of network events in Imhoteph. `Client` handles the event any way it wants to, as of now it displays the output in the terminal.
``` rust
async fn handle_event(event: Event) {
    match event {
        Event::NewListenAddr { addr } => {
            println!("Starting Listening at {}", addr);
        }
        Event::Subscribed { peer_id, topic } => {
            println!("Peer {} joined {}", peer_id, topic)
        }
        Event::Unsubscribed { peer_id, topic } => {
            println!("Peer {} left {}", peer_id, topic)
        }
        Event::Message {
            propagation_source,
            message,
            ..
        } => {
            println!(
                "{}: {}",
                propagation_source,
                String::from_utf8_lossy(&message.data)
            )
        }
        _ => {}
    }
}
```
#  Build from source
 Download [Rust](https://www.rust-lang.org/learn/get-started)
### Clone the repo
 ```shell
 git clone https://github.com/omkar-mohanty/imhoteph.git
 cd imhoteph
 ```
 ### Build Imhoteph
 ```shel
 cargo build 
 ```
 ### Run the application
 ```shell
 cd target/debug
 ./imhoteph-cli
 ```
 
