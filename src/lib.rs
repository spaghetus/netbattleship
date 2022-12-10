#![warn(clippy::pedantic)]

use std::{
	collections::BTreeMap,
	io::{Read, Write},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Default, Clone)]
pub struct Board {
	pub board: BTreeMap<(u8, u8), Ship>,
}

impl Board {
	#[must_use]
	pub fn contains(&self, ship: Ship) -> bool {
		self.board.iter().any(|(_, this_ship)| this_ship == &ship)
	}
}

#[derive(Default, Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Ship {
	#[default]
	None,
	Miss,
	Hit,
	Carrier,
	Battleship,
	Cruiser,
	Submarine,
	Destroyer,
}

impl From<Ship> for char {
	fn from(s: Ship) -> Self {
		match s {
			Ship::None => ' ',
			Ship::Miss => '?',
			Ship::Hit => 'X',
			Ship::Carrier => 'C',
			Ship::Battleship => 'B',
			Ship::Cruiser => 'R',
			Ship::Submarine => 'S',
			Ship::Destroyer => 'D',
		}
	}
}

impl Ship {
	pub fn into_iter() -> std::slice::Iter<'static, Ship> {
		[
			Ship::Carrier,
			Ship::Battleship,
			Ship::Cruiser,
			Ship::Submarine,
			Ship::Destroyer,
		]
		.iter()
	}

	#[must_use]
	pub const fn is_empty(&self) -> bool {
		self.len() == 0
	}

	#[must_use]
	pub const fn len(&self) -> u8 {
		match self {
			Ship::Carrier => 5,
			Ship::Battleship => 4,
			Ship::Cruiser | Ship::Submarine => 3,
			Ship::Destroyer => 2,
			_ => 0,
		}
	}

	pub fn place(&self, board: &mut Board, pos: (u8, u8), v: bool) -> bool {
		if (!v && self.len() > 10 - pos.0) || (v && self.len() > 10 - pos.1) {
			return false;
		}

		let mut cursor = pos;
		for _ in 0..self.len() {
			if board.board.get(&cursor).is_some() {
				return false;
			}
			if v {
				cursor.1 += 1;
			} else {
				cursor.0 += 1;
			}
		}

		let mut cursor = pos;
		for _ in 0..self.len() {
			board.board.insert(cursor, *self);
			if v {
				cursor.1 += 1;
			} else {
				cursor.0 += 1;
			}
		}

		true
	}
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum NetMsg {
	Hello(u64),
	NotFinished,
	Finished,
	DidHit(bool),
	Fire(u8, u8),
	Sunk(Ship),
}

#[derive(Default, Clone)]
pub struct Game {
	pub board: [Board; 2],
	pub turn: bool,
	pub you: bool,
}

impl From<Game> for String {
	fn from(game: Game) -> Self {
		let mut out = String::new();
		out += " | YOU      | THEM     |\n";
		out += " |0123456789|0123456789|\n";
		for row in 0..10 {
			out.push((b'A' + row) as char);
			out += "|";
			let mut left = String::new();
			let mut right = String::new();
			for col in 0..10 {
				left.push(
					game.board[usize::from(game.you)]
						.board
						.get(&(col, row))
						.copied()
						.unwrap_or_default()
						.into(),
				);
				right.push(
					game.board[1 ^ usize::from(game.you)]
						.board
						.get(&(col, row))
						.copied()
						.unwrap_or_default()
						.into(),
				);
			}
			out.push_str(&left);
			out += "|";
			out.push_str(&right);
			out += "|\n";
		}

		out
	}
}

pub fn write_to<T: Serialize, W: Write>(value: &T, into: &mut W) {
	let d = serde_cbor::to_vec(value).expect("bad ser");
	into.write_all(&d).expect("bad write");
	into.write_all(b"\n").expect("bad write");
}

/// # Panics
/// Panics if the struct sent by the other player is not a valid `NetMsg`
pub fn read_from<T: DeserializeOwned, R: Read>(from: &mut R) -> T {
	let d: Vec<u8> = from.bytes().flatten().take_while(|c| *c != b'\n').collect();
	serde_cbor::from_slice(&d).unwrap()
}

#[must_use]
pub fn parse_coord(c: &str) -> Option<(u8, u8)> {
	if let [y, x] = &c.chars().take(2).collect::<Vec<_>>()[..] {
		let y = y.to_ascii_uppercase();
		if !('A'..='J').contains(&y) || !('0'..='9').contains(x) {
			return None;
		}
		let y = y as u8 - b'A';
		let x = *x as u8 - b'0';
		Some((x, y))
	} else {
		None
	}
}
