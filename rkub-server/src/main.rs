use log::*;

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};

use rkub_common::{ClientMessage, Coord, Game, Piece, ServerMessage};

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
    active_delta: i8,
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
            active_delta: 0,
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
                let idx = self.connections[&addr];
                self.players[idx].connected = false;
                info!("[{}] {} closed", addr, self.players[idx].name);

                let _ = self.broadcast(ServerMessage::PlayerDisconnected(idx)).await;

                if self.players.iter().all(|p| !p.connected) {
                    return false;
                }

                if self.active_player == idx {
                    while !self.players[self.active_player].connected {
                        self.active_player = (self.active_player + 1) % self.players.len();
                    }

                    let next_player = &mut self.players[self.active_player];
                    next_player.send_msg(ServerMessage::StartTurn).await;

                    let msg = ServerMessage::TurnFinished {
                        ending_player: self.players[idx].name.clone(),
                        ending_drew: false,
                        next_player: self.active_player,
                        pieces_remaining: self.game.remaining_pieces().len(),
                        board: self.game.board().clone(),
                    };

                    let _ = self.broadcast(msg).await;
                }
            }
            ClientMessage::EndTurn => {
                if self.connections[&addr] != self.active_player {
                    info!(
                        "[{}] player tried to make a turn when it wasn't their turn",
                        addr
                    );
                    return true;
                }

                let (is_valid, groups) = self.game.is_valid_board();
                info!("[{}] valid play? {}, groups: {:?}", addr, is_valid, groups);

                if !is_valid {
                    let msg = ServerMessage::InvalidBoardState;
                    self.players[self.connections[&addr]].send_msg(msg).await;
                    return true;
                }
                info!(
                    "[{}] {} valid turn: delta: {}",
                    addr, self.players[self.connections[&addr]].name, self.active_delta
                );

                let mut drew = self.active_delta == 0;
                if drew {
                    if let Some(piece) = self.game.deal_piece() {
                        let msg = ServerMessage::DrawPiece(piece);
                        self.players[self.connections[&addr]].send_msg(msg).await;
                    } else {
                        drew = false;
                    }
                }

                if !drew && self.players[self.connections[&addr]].hand.is_empty() {
                    info!(
                        "[{}] {} won the game!",
                        addr, self.players[self.connections[&addr]].name
                    );

                    let _ = self
                        .broadcast(ServerMessage::PlayerWon(
                            self.players[self.connections[&addr]].name.clone(),
                        ))
                        .await;
                    return false;
                }

                let msg = ServerMessage::EndTurnValid;
                self.players[self.connections[&addr]].send_msg(msg).await;

                info!(
                    "[{}] {} hand length: {}",
                    addr,
                    self.players[self.connections[&addr]].name,
                    self.players[self.connections[&addr]].hand.len()
                );

                self.active_delta = 0;

                let ending_player = self.players[self.connections[&addr]].name.clone();
                self.active_player = (self.active_player + 1) % self.players.len();

                while !self.players[self.active_player].connected {
                    self.active_player = (self.active_player + 1) % self.players.len();
                }

                let next_player = &mut self.players[self.active_player];
                next_player.send_msg(ServerMessage::StartTurn).await;

                let msg = ServerMessage::TurnFinished {
                    ending_player,
                    ending_drew: drew,
                    next_player: self.active_player,
                    pieces_remaining: self.game.remaining_pieces().len(),
                    board: self.game.board().clone(),
                };

                let _ = self.broadcast(msg).await;
            }
            ClientMessage::Pickup(coord, piece) => {
                if self.connections[&addr] != self.active_player {
                    info!(
                        "[{}] player tried to make a turn when it wasn't their turn",
                        addr
                    );
                    return true;
                }

                info!("[{}] pickup: {:?} {:?}", addr, coord, piece);
                let _ = self.game.board_mut().remove(&coord);

                let player = &mut self.players[self.connections[&addr]];
                player.hand.push(piece);

                self.active_delta -= 1;

                let _ = self.broadcast(ServerMessage::Pickup(coord, piece)).await;
            }
            ClientMessage::Place(coord, piece) => {
                if self.connections[&addr] != self.active_player {
                    info!(
                        "[{}] player tried to make a turn when it wasn't their turn",
                        addr
                    );
                    return true;
                }

                info!("[{}] place: {:?} {:?}", addr, coord, piece);
                self.game.board_mut().insert(coord, piece);
                self.active_delta += 1;

                let player = &mut self.players[self.connections[&addr]];

                for i in 0..player.hand.len() {
                    if player.hand[i] == piece {
                        player.hand.swap_remove(i);
                        break;
                    }
                }

                let _ = self.broadcast(ServerMessage::Place(coord, piece)).await;
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

        if let Some((idx, _)) = self
            .players
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == name && !p.connected)
        {
            self.connections.insert(addr, idx);
        }

        if self.connections.contains_key(&addr) {
            info!("[{}] {} reconnected!", addr, name);
            self.players[self.connections[&addr]].connected = true;
            let hand = self.players[self.connections[&addr]].hand.clone();

            let pieces_remaining = self.game.remaining_pieces().len();
            ws_sender
                .send(ServerMessage::JoinedRoom {
                    room_name: self.name.clone(),
                    players: self.players.iter().map(|p| p.name.clone()).collect(),
                    hand: hand.clone(),
                    pieces_remaining,
                    board: self.game.board().clone(),
                })
                .await?;

            ws_sender
                .send(ServerMessage::CurrentPlayer(self.active_player))
                .await?;

            self.players[self.connections[&addr]].sender = ws_sender;
            let _ = self
                .broadcast(ServerMessage::PlayerReconnected(self.connections[&addr]))
                .await;

            return Ok(());
        }

        // let hand = self.game.deal(14);

        let hand = self.game.deal(28);
        let player = Player::new(name.to_string(), hand.clone(), ws_sender.clone());

        self.broadcast(ServerMessage::PlayerJoined(name.to_string()))
            .await?;

        self.players.push(player);

        let pieces_remaining = self.game.remaining_pieces().len();
        ws_sender
            .send(ServerMessage::JoinedRoom {
                room_name: self.name.clone(),
                players: self.players.iter().map(|p| p.name.clone()).collect(),
                hand,
                pieces_remaining,
                board: self.game.board().clone(),
            })
            .await?;

        self.connections.insert(addr, self.players.len() - 1);

        Ok(())
    }

    pub async fn broadcast(&self, msg: ServerMessage) -> anyhow::Result<()> {
        for idx in self.connections.values() {
            if self.players[*idx].connected {
                self.players[*idx].sender.send(msg.clone()).await?;
            }
        }

        Ok(())
    }
}

type Rooms = Lock<HashMap<String, RoomHandle>>;

pub struct Player {
    name: String,
    connected: bool,
    hand: Vec<Piece>,
    sender: Sender<ServerMessage>,
}

impl Player {
    pub fn new(name: String, hand: Vec<Piece>, sender: Sender<ServerMessage>) -> Self {
        Self {
            name,
            connected: true,
            hand,
            sender,
        }
    }

    pub async fn send_msg(&mut self, msg: ServerMessage) {
        let _ = self.sender.send(msg).await;
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
                _ => {}
            }
        }

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

                info!("Creating a new ID...");

                let new_id = {
                    info!("Locking room");
                    let map = rooms.lock().await;
                    info!("Room locked");
                    new_room_and_id(map, handle.clone()).await
                };

                info!("created new room: {}", new_id);

                let (_, res) = join!(
                    run_room(handle.clone(), recv),
                    run_player(addr, name, ws, handle)
                );

                res?;

                info!("finished running room: {}", new_id);

                return Ok(());

                // TODO: remove room
            }
            ClientMessage::JoinRoom(player_name, room) => {
                info!("[{}] {} joined {}", addr, player_name, room);

                let handle = { rooms.lock().await.get(&room).cloned() };

                if let Some(room_handle) = handle {
                    run_player(addr, player_name, ws, room_handle).await?;
                } else {
                    // TODO: Handle error case
                    error!("[{}] room {}: could not be found", addr, room);
                }

                return Ok(());
            }
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
                .filter(char::is_ascii_lowercase)
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
