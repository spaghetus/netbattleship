#![warn(clippy::pedantic)]

use std::default;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use ship::Ship;

pub mod board;
pub mod ship;

#[derive(Default, Clone)]
pub struct Game {
	pub board: [board::Board; 2],
	pub turn: bool,
	pub you: bool,
	pub phase: Phase,
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

#[derive(Default, Clone)]
pub enum Phase {
	#[default]
	Connecting,
	Placing(Ship),
	Playing,
	Done,
}

pub mod net;

pub mod ui;

pub mod flow;
