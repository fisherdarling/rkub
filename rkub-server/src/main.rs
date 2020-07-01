use log::*;

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};

use rkub_common::{ClientMessage, Game, Piece, ServerMessage};

use async_channel::{unbounded, Receiver, Sender};
use async_lock::{Lock, LockGuard};
use futures::{join, SinkExt, StreamExt};
use smol::Async;

use async_tungstenite::{accept_async, WebSocketStream};
use tungstenite::Message;

type TaggedClientMessage = (SocketAddr, ClientMessage);

#[derive(Clone)]
struct RoomHandle {
    pub send: Sender<TaggedClientMessage>,
    pub room: Lock<Room>,
}

async fn run_room(handle: RoomHandle, mut read: Receiver<TaggedClientMessage>) {
    info!("Running Room: {}", handle.room.lock().await.name);
    while let Some((addr, msg)) = read.next().await {
        if !handle.room.lock().await.on_message(addr, msg) {
            break;
        }
    }
}

#[derive(Default)]
struct Room {
    name: String,
    started: bool,
    ended: bool,
    connections: HashMap<SocketAddr, usize>,
    players: Vec<Player>,
    active_player: usize,
    game: Game,
}

impl Room {
    pub fn has_started(&self) -> bool {
        self.started
    }

    pub fn on_message(&mut self, addr: SocketAddr, msg: ClientMessage) -> bool {
        info!("[{}] message: {:?}", addr, msg);

        let player = &self.players[self.connections[&addr]];

        match msg {
            ClientMessage::Ping => {
                player.sender.send(ServerMessage::Pong);
            }
            _ => {},
        }

        true
    }

    pub async fn add_player(
        &mut self,
        addr: SocketAddr,
        name: &str,
        ws_sender: Sender<ServerMessage>,
    ) -> anyhow::Result<()> {
        if self.has_started() {
            ws_sender
                .send(ServerMessage::GameAlreadyStarted(self.name.clone()))
                .await?;
        }

        let hand = self.game.deal(14);
        let player = Player::new(name.to_string(), hand, ws_sender.clone());

        self.broadcast(ServerMessage::PlayerJoined(name.to_string()))
            .await?;

        ws_sender
            .send(ServerMessage::JoinedRoom {
                room_name: self.name.clone(),
                players: self.players.iter().map(|p| p.name.clone()).collect(),
                pieces: player.pieces.clone(),
            })
            .await?;

        self.players.push(player);
        self.connections.insert(addr, self.players.len() - 1);

        Ok(())
    }

    pub async fn broadcast(&self, msg: ServerMessage) -> anyhow::Result<()> {
        for idx in self.connections.values() {
            self.players[*idx].sender.send(msg.clone()).await?;
        }

        Ok(())
    }
}

type Rooms = Lock<HashMap<String, RoomHandle>>;

pub struct Player {
    name: String,
    pieces: Vec<Piece>,
    sender: Sender<ServerMessage>,
}

impl Player {
    pub fn new(name: String, pieces: Vec<Piece>, sender: Sender<ServerMessage>) -> Self {
        Self {
            name,
            pieces: Vec::new(),
            sender,
        }
    }
}

async fn run_player(
    addr: SocketAddr,
    name: String,
    stream: WebSocketStream<Async<TcpStream>>,
    handle: RoomHandle,
) -> anyhow::Result<()> {
    let (incoming, outgoing) = stream.split();
    let (ws_tx, ws_rx) = unbounded();

    {
        let mut room = handle.room.lock().await;
        room.add_player(addr, &name, ws_tx).await?;
    }

    Ok(())
}

async fn handle_connection(
    stream: Async<TcpStream>,
    addr: SocketAddr,
    rooms: Rooms,
) -> anyhow::Result<()> {
    info!("[{}] incoming connection", addr);

    let mut ws = accept_async(stream).await?;

    while let Some(Ok(Message::Text(t))) = ws.next().await {
        let message: ClientMessage = serde_json::from_str(&t)?;

        match message {
            ClientMessage::Ping => {
                info!("[{}] {:?}", addr, ClientMessage::Ping);
                ws.send(Message::Text(serde_json::to_string(&ServerMessage::Pong)?))
                    .await?;
            }
            ClientMessage::CreateRoom(name) => {
                info!("[{}] creating room for: {}", addr, name);

                // Create send and receive queues for this room / player:
                let (send, recv) = unbounded();

                // Create a new room and get its id:
                let room = Lock::new(Room::default());
                let handle = RoomHandle { send, room };

                let new_id = {
                    let map = rooms.lock().await;
                    new_room_and_id(map, handle.clone()).await
                };

                info!("created new room: {}", new_id);

                join!(
                    run_room(handle.clone(), recv),
                    run_player(addr, name, ws, handle)
                );

                return Ok(());

                // TODO: remove room
            }
            ClientMessage::JoinRoom(name, room) => {}
            _ => {
                error!("Unexpected Message from {}", addr);
            }
        }
    }

    Ok(())
}

async fn new_room_and_id(
    mut map: LockGuard<HashMap<String, RoomHandle>>,
    handle: RoomHandle,
) -> String {
    use rand::distributions::Alphanumeric;
    use rand::{thread_rng, Rng};
    use std::iter;

    // let mut map = rooms.await;
    loop {
        let new_id: String = {
            let mut rng = thread_rng();
            iter::repeat(())
                .map(|_| rng.sample(Alphanumeric))
                .filter(char::is_ascii_alphabetic)
                .take(6)
                .collect()
        };

        if map.contains_key(&new_id) {
            continue;
        }

        let mut room = handle.room.lock().await;
        room.name = new_id.clone();
        map.insert(new_id.clone(), handle);

        break new_id;
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::try_init()?;

    info!("Server Starting");

    // Create our thread pool:
    for _ in 0..4 {
        std::thread::spawn(|| smol::run(futures::future::pending::<()>()));
    }

    let addr = "127.0.0.1:5555".to_string();
    let rooms = Rooms::default();

    smol::block_on(async {
        let listener = Async::<TcpListener>::bind(&addr).unwrap();

        info!("Binding to: {}", addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let rc = rooms.clone();
            smol::Task::spawn(async move {
                if let Err(e) = handle_connection(stream, addr, rc).await {
                    eprintln!("error: {}", e);
                }
            })
            .detach();
        }
    });

    Ok(())
}
