# imhoteph
A peer to peer chat application using libp2p Swarm and Gossipsub protocols. 
# Motivation
Almost all major instant messaging apps are made by big tech who have a less than stellar track record of respecting user privacy. Imhoteph aims to be reliable, scalable and at the same time respect the privacy of the user. 
# Tech used
- Rust nightly
- Libp2p
- Clap
- Tokio
# Usage 
`imhoteph-cli [SUBCOMMAND]`
## Subcommands
- `dial --id [PEER ID] --topic [TOPIC NAME]`
  Dial the given Peer ID and subscribe to the topic 
- `test --id [PEER ID] --addr [PEER MULTIADDR]`
  Test dial a the given peer id with the given multiaddr.
# Working 
## Initialization
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
## Message Passing
The `Eventloop` struct has `event_sender` field of type `mpsc::channel` which is used to pass messages to other the corespinding `` 

