use crate::ship::Ship;
use std::collections::BTreeMap;

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
