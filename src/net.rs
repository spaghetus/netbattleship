use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::io::Read;
use std::io::Write;

use crate::ship;
use crate::ship::Ship;

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

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Msg {
	Hello(u64),
	NotFinished,
	Finished,
	DidHit(bool),
	Fire(u8, u8),
	Sunk(Ship),
}
