use std::{fmt, net::SocketAddr};

use thiserror::Error;
use tokio::{
	io::AsyncWriteExt,
	net::{TcpListener, TcpStream},
};

use crate::{
	net::{read_from_async, write_to, write_to_async, Msg},
	ship::Ship,
	Game, Phase,
};

pub struct GameFlow {
	state: Game,
	socket: TcpStream,
}

#[derive(Error, Debug)]
pub enum GameFlowError {
	Network(#[from] tokio::io::Error),
	BadMessage(Msg),
	InvalidPlacement,
	OutOfOrder,
	MalformedMessage(#[from] serde_cbor::Error),
}

impl fmt::Display for GameFlowError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
impl GameFlow {
	pub async fn new(addr: &SocketAddr, serve: bool) -> Result<GameFlow, GameFlowError> {
		let socket = Self::handshake(addr, serve).await?;

		Ok(GameFlow {
			state: Game {
				you: serve,
				phase: Phase::Placing(*Ship::into_iter().next().unwrap()),
				..Default::default()
			},
			socket,
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

	pub fn my_turn(&self) -> bool {
		self.state.turn == self.state.you
	}

	pub fn phase(&self) -> Phase {
		self.state.phase.clone()
	}

	pub async fn place_ship(
		&mut self,
		ship: Ship,
		pos: (u8, u8),
		v: bool,
	) -> Result<(), GameFlowError> {
		match self.phase() {
			Phase::Placing(s) if s == ship => {}
			_ => return Err(GameFlowError::OutOfOrder),
		}

		if ship.place(&mut self.state.board[usize::from(self.state.you)], pos, v) {
			self.state.phase = Ship::into_iter()
				.skip_while(|&&a| match self.state.phase {
					Phase::Placing(b) => a != b,
					_ => unreachable!(),
				})
				.map(|v| Phase::Placing(*v))
				.next()
				.unwrap_or(Phase::Playing);
			if self.state.phase == Phase::Playing {
				write_to_async(&Msg::Finished, &mut self.socket).await;
			}
			Ok(())
		} else {
			Err(GameFlowError::InvalidPlacement)
		}
	}

	pub async fn fire(&mut self, pos: (u8, u8)) -> Result<TurnResults, GameFlowError> {
		if self.phase() != Phase::Playing || !self.my_turn() {
			return Err(GameFlowError::OutOfOrder);
		}
		// Send the fire message
		write_to_async(&Msg::Fire(pos.0, pos.1), &mut self.socket).await;
		// Did we hit?
		let hit = match read_from_async(&mut self.socket).await {
			Msg::DidHit(b) => b,
			m => return Err(GameFlowError::BadMessage(m)),
		};
		// Did we sink?
		let sunk = match read_from_async(&mut self.socket).await {
			Msg::Sunk(Ship::None) => None,
			Msg::Sunk(s) => Some(s),
			m => return Err(GameFlowError::BadMessage(m)),
		};
		// Did we win?
		let won = match read_from_async(&mut self.socket).await {
			Msg::Finished => true,
			Msg::NotFinished => false,
			m => return Err(GameFlowError::BadMessage(m)),
		};
		Ok(TurnResults {
			hit: Some(Ship::Hit).filter(|_| hit),
			sunk,
			won,
		})
	}

	pub async fn receive(&mut self) -> Result<TurnResults, GameFlowError> {
		if self.phase() != Phase::Playing || self.my_turn() {
			return Err(GameFlowError::OutOfOrder);
		}

		let aim = match read_from_async(&mut self.socket).await {
			Msg::Fire(x, y) => (x, y),
			m => return Err(GameFlowError::BadMessage(m)),
		};

		let hit = self.state.board[usize::from(self.state.you)]
			.board
			.insert(aim, Ship::Hit)
			.filter(|v| !matches!(v, Ship::Hit | Ship::None | Ship::Miss));
		write_to_async(&Msg::DidHit(hit.is_some()), &mut self.socket).await;

		let sunk = hit.filter(|hit| !self.state.board[usize::from(self.state.you)].contains(*hit));
		write_to_async(&Msg::Sunk(sunk.unwrap_or(Ship::None)), &mut self.socket).await;

		let won = self.state.board[usize::from(self.state.you)]
			.board
			.iter()
			.all(|(_, ship)| ship.is_empty());
		write_to_async(
			&if won { Msg::Finished } else { Msg::NotFinished },
			&mut self.socket,
		)
		.await;

		Ok(TurnResults { hit, sunk, won })
	}

	pub async fn done(mut self) -> Result<(), GameFlowError> {
		self.socket.shutdown().await?;
		Ok(())
	}
}

pub struct TurnResults {
	pub hit: Option<Ship>,
	pub sunk: Option<Ship>,
	pub won: bool,
}
