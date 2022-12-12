use serde::{Deserialize, Serialize};

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

	pub fn place(&self, board: &mut crate::board::Board, pos: (u8, u8), v: bool) -> bool {
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
