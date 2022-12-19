use std::{fmt, net::SocketAddr, sync::Arc};

use thiserror::Error;
use tokio::{
	io::AsyncWriteExt,
	net::{TcpListener, TcpStream},
	sync::RwLock,
};

use crate::{
	board::Board,
	net::{read_from_async, write_to_async, Msg},
	ship::Ship,
	Game, Phase,
};

#[allow(clippy::module_name_repetitions)]
pub struct GameFlow {
	pub state: Arc<RwLock<Game>>,
	pub socket: Arc<RwLock<TcpStream>>,
}

#[derive(Error, Debug)]
pub enum GameFlowError {
	Network(#[from] tokio::io::Error),
	BadMessage(Msg),
	InvalidPlacement,
	OutOfOrder,
	MalformedMessage(#[from] serde_cbor::Error),
	Mismatch(u64, u64),
	Busy(#[from] std::sync::TryLockError<()>),
}

impl fmt::Display for GameFlowError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

const VERSION: u64 = 1;

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
impl GameFlow {
	pub async fn new(addr: SocketAddr, serve: bool) -> Result<GameFlow, GameFlowError> {
		let mut socket = Self::handshake(&addr, serve).await?;

		write_to_async(&Msg::Hello(VERSION), &mut socket).await;
		match read_from_async(&mut socket).await {
			Msg::Hello(other) => {
				if other != VERSION {
					return Err(GameFlowError::Mismatch(VERSION, other));
				}
			}
			m => return Err(GameFlowError::BadMessage(m)),
		}

		Ok(GameFlow {
			state: Arc::new(RwLock::new(Game {
				you: serve,
				turn: true,
				phase: Phase::Placing(*Ship::into_iter().next().unwrap()),
				..Default::default()
			})),
			socket: Arc::new(RwLock::new(socket)),
		})
	}

	async fn handshake(addr: &SocketAddr, serve: bool) -> Result<TcpStream, GameFlowError> {
		if serve {
			let listen = TcpListener::bind(addr).await?;
			Ok(listen.accept().await?.0)
		} else {
			Ok(TcpStream::connect(addr).await?)
		}
	}

	pub async fn my_turn(&self) -> bool {
		let state = self.state.read().await;
		state.turn == state.you
	}

	pub async fn phase(&self) -> Phase {
		self.state.read().await.phase.clone()
	}

	pub async fn place_ship(
		&self,
		ship: Ship,
		pos: (u8, u8),
		v: bool,
	) -> Result<(), GameFlowError> {
		match self.phase().await {
			Phase::Placing(s) if s == ship => {}
			_ => return Err(GameFlowError::OutOfOrder),
		}

		let mut state = self.state.write().await;
		let you = state.you;

		if ship.place(&mut state.board[usize::from(you)], pos, v) {
			state.phase = Ship::into_iter()
				.skip_while(|&&a| match state.phase {
					Phase::Placing(b) => a != b,
					_ => unreachable!(),
				})
				.skip(1)
				.map(|v| Phase::Placing(*v))
				.next()
				.unwrap_or(Phase::Playing);
			Ok(())
		} else {
			Err(GameFlowError::InvalidPlacement)
		}
	}

	pub async fn fire(&self, pos: (u8, u8)) -> Result<TurnResults, GameFlowError> {
		if self.phase().await != Phase::Playing || !self.my_turn().await {
			return Err(GameFlowError::OutOfOrder);
		}

		let mut socket = self.socket.write().await;

		// Send the fire message
		write_to_async(&Msg::Fire(pos.0, pos.1), &mut *socket).await;
		// Did we hit?
		let hit = match read_from_async(&mut *socket).await {
			Msg::DidHit(b) => b,
			m => return Err(GameFlowError::BadMessage(m)),
		};

		let mut state = self.state.write().await;
		let you = state.you;

		// Place the hit or miss marker
		state.board[usize::from(!you)]
			.board
			.insert(pos, if hit { Ship::Hit } else { Ship::Miss });

		// Did we sink?
		let sunk = match read_from_async(&mut *socket).await {
			Msg::Sunk(Ship::None) => None,
			Msg::Sunk(s) => Some(s),
			m => return Err(GameFlowError::BadMessage(m)),
		};
		// Did we win?
		let won = match read_from_async(&mut *socket).await {
			Msg::Finished => {
				state.phase = Phase::Done(true);
				true
			}
			Msg::NotFinished => false,
			m => return Err(GameFlowError::BadMessage(m)),
		};
		state.turn = !state.turn;
		Ok(TurnResults {
			hit: Some(Ship::Hit).filter(|_| hit),
			sunk,
			won,
			aim: pos,
		})
	}

	pub async fn receive(&self) -> Result<TurnResults, GameFlowError> {
		if self.phase().await != Phase::Playing || self.my_turn().await {
			return Err(GameFlowError::OutOfOrder);
		}

		let aim = match read_from_async(&mut *self.socket.write().await).await {
			Msg::Fire(x, y) => (x, y),
			m => return Err(GameFlowError::BadMessage(m)),
		};

		let mut state = self.state.write().await;
		let you = state.you;

		let hit = state.board[usize::from(you)]
			.board
			.insert(aim, Ship::Hit)
			.filter(|v| !matches!(v, Ship::Hit | Ship::None | Ship::Miss));
		write_to_async(&Msg::DidHit(hit.is_some()), &mut *self.socket.write().await).await;

		let sunk = hit.filter(|hit| !state.board[usize::from(you)].contains(*hit));
		write_to_async(
			&Msg::Sunk(sunk.unwrap_or(Ship::None)),
			&mut *self.socket.write().await,
		)
		.await;

		let won = state.board[usize::from(you)]
			.board
			.iter()
			.all(|(_, ship)| ship.is_empty());
		write_to_async(
			&if won { Msg::Finished } else { Msg::NotFinished },
			&mut *self.socket.write().await,
		)
		.await;

		if won {
			state.phase = Phase::Done(false);
		}

		state.turn = !state.turn;
		Ok(TurnResults {
			aim,
			hit,
			sunk,
			won,
		})
	}

	pub async fn done(self) -> Result<(), GameFlowError> {
		self.socket.write().await.shutdown().await?;
		Ok(())
	}

	pub async fn to_string(&self) -> String {
		String::from(self.state.read().await.clone())
	}

	pub async fn board(&self, enemy: bool) -> Board {
		let state = self.state.read().await;
		let you = state.you;
		state.board[usize::from(you ^ enemy)].clone()
	}
}

pub struct TurnResults {
	pub aim: (u8, u8),
	pub hit: Option<Ship>,
	pub sunk: Option<Ship>,
	pub won: bool,
}
