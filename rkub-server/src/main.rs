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
        if !handle.room.lock().await.on_message(addr, msg).await {
            break;
        }
    }
}
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
    pub fn new() -> Self {
        let game = Game::new();

        Room {
            name: String::new(),
            started: false,
            ended: false,
            connections: HashMap::new(),
            players: Vec::new(),
            active_player: 0,
            game,
        }
    }

    pub fn has_started(&self) -> bool {
        self.started
    }

    pub async fn on_message(&mut self, addr: SocketAddr, msg: ClientMessage) -> bool {
        info!("[{}] message: {:?}", addr, msg);

        let player = &self.players[self.connections[&addr]];

        match msg {
            ClientMessage::Ping => {
                if let Err(_) = player.sender.send(ServerMessage::Pong).await {
                    panic!("Error sending to player");
                }
            }
            ClientMessage::Close => {
                info!("[{}] closed", addr);
                // let idx = self.connections.remove(&addr).unwrap();
                // self.players.remove(idx);
            }
            ClientMessage::PlayedPieces(board, mut hand) => {                
                if self.connections[&addr] != self.active_player {
                    info!("[{}] player tried to make a turn when it wasn't their turn", addr);
                    return true;
                }
                
                let (is_valid, groups) = Game::is_valid_board(&board);
                info!("[{}] valid play? {}, groups: {}", addr, is_valid, groups.len());

                if !is_valid {
                    let msg = ServerMessage::InvalidPlay(board);
                    self.players[self.connections[&addr]].send_msg(msg).await;
                    
                    return true;
                }

                let player = &mut self.players[self.connections[&addr]];
                
                hand.sort();
                player.hand_mut().sort();

                // There was no hand change, player has to draw a piece:
                let mut drew = false;
                if hand == *player.hand_mut() {
                    if let Some(piece) = self.game.deal_piece() {
                        player.add_to_hand(piece);
                        player.send_msg(ServerMessage::DrawPiece(piece)).await;
                        drew = true;
                    }
                }
                let ending_player = player.name.clone();

                self.game.set_board(board.clone());
                self.active_player = (self.active_player + 1) % self.players.len();
                let next_player = &self.players[self.active_player];

                let msg = ServerMessage::TurnFinished {
                    ending_player,
                    ending_drew: drew,
                    next_player: next_player.name.clone(),
                    pieces_remaining: self.game.remaining_pieces().len(),
                    board,
                };

                self.broadcast(msg).await;
            }
            _ => {}
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

        // let hand = self.game.deal(14);
        
        let hand = self.game.deal(28);
        let player = Player::new(name.to_string(), hand.clone(), ws_sender.clone());

        self.broadcast(ServerMessage::PlayerJoined(name.to_string()))
            .await?;

        self.players.push(player);

        ws_sender
            .send(ServerMessage::JoinedRoom {
                room_name: self.name.clone(),
                players: self.players.iter().map(|p| p.name.clone()).collect(),
                hand,
            })
            .await?;

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
    hand: Vec<Piece>,
    sender: Sender<ServerMessage>,
}

impl Player {
    pub fn new(name: String, hand: Vec<Piece>, sender: Sender<ServerMessage>) -> Self {
        Self {
            name,
            hand,
            sender,
        }
    }

    pub async fn send_msg(&mut self, msg: ServerMessage) {
        self.sender.send(msg).await;
    }

    pub fn add_to_hand(&mut self, piece: Piece) {
        self.hand.push(piece);
    }

    pub fn hand_mut(&mut self) -> &mut Vec<Piece> {
        &mut self.hand
    }
}

async fn run_player(
    addr: SocketAddr,
    name: String,
    stream: WebSocketStream<Async<TcpStream>>,
    handle: RoomHandle,
) -> anyhow::Result<()> {
    info!("[{}] run player: {}", addr, name);
    
    let (mut outgoing, mut incoming) = stream.split();
    let (ws_tx, ws_rx) = unbounded();

    {
        let mut room = handle.room.lock().await;
        room.add_player(addr, &name, ws_tx).await?;
    }

    let server_to_client: smol::Task<anyhow::Result<()>> = smol::Task::spawn(async move {
        while let Ok(message) = ws_rx.recv().await {
            let json = serde_json::to_string(&message)?;
            outgoing.send(Message::Text(json)).await?;
        }

        Ok(())
    });

    let server_write = handle.send.clone();
    let client_to_server: smol::Task<anyhow::Result<()>> = smol::Task::spawn(async move {
        while let Some(message) = incoming.next().await.transpose()? {
            match message {
                Message::Text(json) => {
                    let message: ClientMessage = serde_json::from_str(&json)?;
                    server_write.send((addr, message)).await;
                }
                _ => {},
            }
        };

        server_write.send((addr, ClientMessage::Close)).await;

        Ok(())
    });

    info!("[{}] joining streams for: {}", addr, name);
    let (_s2c_e, _c2s_e) = join!(server_to_client, client_to_server);
    info!("[{}] finished streams for: {}", addr, name);

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
                let room = Lock::new(Room::new());
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
