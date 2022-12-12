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
