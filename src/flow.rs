use crate::net::read_from;
use crate::net::write_to;
use crate::net::Msg;
use crate::ship::Ship;
use crate::Phase;

use super::Game;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::RwLock;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub enum UiMsg {
	Fire(u8, u8),
	PlaceShip(u8, u8, bool),
	Sunk(Ship),
	DidHit(bool, Option<Ship>),
	Won(bool),
	Say(String),
	NewPhase,
	NoConnect,
}

pub struct GameFlowServer {
	pub tx: Sender<UiMsg>,
	pub rx: Receiver<UiMsg>,
	pub state: Arc<RwLock<Game>>,
	pub(crate) _thread: JoinHandle<()>,
}

impl GameFlowServer {
	pub(crate) fn game_flow(
		mut tx: Sender<UiMsg>,
		mut rx: Receiver<UiMsg>,
		state: Arc<RwLock<Game>>,
		addr: SocketAddr,
		host: bool,
	) {
		// Connection phase
		state.write().unwrap().phase = Phase::Connecting;
		tx.send(UiMsg::NewPhase).unwrap();
		let mut stream = handshake(&addr, host);
		ship_placement(&state, &mut tx, &mut rx, &mut stream);
		assert_eq!(Msg::Finished, read_from(&mut stream));
		state.write().unwrap().phase = Phase::Playing;
		tx.send(UiMsg::NewPhase).unwrap();
		game_loop(&state, &mut tx, &mut rx, &mut stream);
		state.write().unwrap().phase = Phase::Done;
		tx.send(UiMsg::NewPhase).unwrap();
		stream
			.shutdown(std::net::Shutdown::Both)
			.expect("Bad shutdown");
	}

	#[must_use]
	pub fn new(addr: SocketAddr, host: bool) -> GameFlowServer {
		let (ui_send, flow_recv) = mpsc::channel();
		let (flow_send, ui_recv) = mpsc::channel();
		let state = Arc::new(RwLock::new(Game {
			you: !host,
			..Default::default()
		}));

		GameFlowServer {
			tx: ui_send,
			rx: ui_recv,
			state: state.clone(),
			_thread: thread::spawn(move || {
				GameFlowServer::game_flow(flow_send, flow_recv, state.clone(), addr, host);
			}),
		}
	}
}

fn handshake(addr: &SocketAddr, serve: bool) -> TcpStream {
	let mut stream = if serve {
		println!("Listening on {}", addr);
		let list = TcpListener::bind(addr).expect("Failed to listen.");
		list.accept().expect("Failed to receive connection.").0
	} else {
		println!("Connecting to {}", addr);
		TcpStream::connect_timeout(addr, Duration::from_secs(10)).expect("Failed to connect.")
	};
	println!("Got connection...");
	let msg = Msg::Hello(0);
	write_to(&msg, &mut stream);
	let resp: Msg = read_from(&mut stream);
	assert_eq!(msg, resp);
	println!("Got good handshake!");
	stream
}

fn ship_placement(
	state: &Arc<RwLock<Game>>,
	tx: &mut Sender<UiMsg>,
	rx: &mut Receiver<UiMsg>,
	stream: &mut TcpStream,
) {
	for ship_kind in Ship::into_iter() {
		tx.send(UiMsg::NewPhase).unwrap();
		state.write().unwrap().phase = Phase::Placing(ship_kind.clone());
		loop {
			if let UiMsg::PlaceShip(x, y, vertical) = rx.recv().unwrap() {
				let mut game = state.write().unwrap();
				let you = game.you;
				if ship_kind.place(&mut game.board[usize::from(you)], (x, y), vertical) {
					break;
				}
				tx.send(UiMsg::Say("Bad placement, try again.".to_string()))
					.unwrap();
			}
		}
	}
	write_to(&Msg::Finished, stream);
}

fn game_loop(
	state: &Arc<RwLock<Game>>,
	tx: &mut Sender<UiMsg>,
	rx: &mut Receiver<UiMsg>,
	stream: &mut TcpStream,
) {
	tx.send(UiMsg::NewPhase).unwrap();
	loop {
		if state.read().unwrap().turn == state.read().unwrap().you {
			// Choose a target
			let coords = if let UiMsg::Fire(x, y) = rx.recv().unwrap() {
				(x, y)
			} else {
				continue;
			};
			// Fire
			write_to(&Msg::Fire(coords.0, coords.1), stream);
			// Did we hit?
			let resp = read_from(stream);
			match resp {
				Msg::DidHit(false) => {
					tx.send(UiMsg::DidHit(false, None)).unwrap();
					let mut game = state.write().unwrap();
					let you = game.you;
					game.board[1 ^ usize::from(you)]
						.board
						.insert(coords, Ship::Miss);
				}
				Msg::DidHit(true) => {
					tx.send(UiMsg::DidHit(true, None)).unwrap();
					let mut game = state.write().unwrap();
					let you = game.you;
					game.board[1 ^ usize::from(you)]
						.board
						.insert(coords, Ship::Hit);
				}
				m => panic!("Unexpected {:?}", m),
			}
			// Did we sink?
			let resp = read_from(stream);
			match resp {
				Msg::Sunk(Ship::None) => {}
				Msg::Sunk(s) => tx.send(UiMsg::Sunk(s)).unwrap(),
				m => panic!("Unexpected {:?}", m),
			}
			// Is the game over?
			let resp = read_from(stream);
			match resp {
				Msg::Finished => {
					tx.send(UiMsg::Won(true)).unwrap();
					break;
				}
				Msg::NotFinished => {}
				m => panic!("Unexpected {:?}", m),
			}
		} else {
			// Enemy's turn.
			let msg = read_from(stream);
			let aim = match msg {
				Msg::Fire(x, y) => (x, y),
				m => panic!("Unexpected {:?}", m),
			};
			tx.send(UiMsg::Fire(aim.0, aim.1)).unwrap();
			// Did the enemy hit the ship?
			let mut game = state.write().unwrap();
			let you = game.you;
			let hit = game.board[usize::from(you)]
				.board
				.insert(aim, Ship::Hit)
				.filter(|v| !matches!(v, Ship::Hit | Ship::None | Ship::Miss))
				.unwrap_or(Ship::None);
			match hit {
				Ship::None => {
					write_to(&Msg::DidHit(false), stream);
					tx.send(UiMsg::DidHit(false, None)).unwrap();
				}
				s => {
					write_to(&Msg::DidHit(true), stream);
					tx.send(UiMsg::DidHit(true, Some(s))).unwrap();
				}
			}
			// Did the enemy sink our ship?
			if !matches!(hit, Ship::None) && !game.board[usize::from(game.you)].contains(hit) {
				tx.send(UiMsg::Sunk(hit)).unwrap();
				write_to(&Msg::Sunk(hit), stream);
			} else {
				write_to(&Msg::Sunk(Ship::None), stream);
			}
			// Did we lose?
			if game.board[usize::from(game.you)]
				.board
				.iter()
				.all(|(_, ship)| ship.is_empty())
			{
				tx.send(UiMsg::Won(false)).unwrap();
				write_to(&Msg::Finished, stream);
				break;
			}
			write_to(&Msg::NotFinished, stream);
		}
		state.write().unwrap().turn ^= true;
	}
}
