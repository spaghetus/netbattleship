use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::io::Read;
use std::io::Write;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

use crate::ship::Ship;

pub fn write_to<T: Serialize, W: Write>(value: &T, into: &mut W) {
	let d = serde_cbor::to_vec(value).expect("bad ser");
	into.write_all(&d).expect("bad write");
	into.write_all(b"\n").expect("bad write");
}
pub async fn write_to_async<T: Serialize, W: AsyncWrite + AsyncWriteExt + Unpin>(
	value: &T,
	into: &mut W,
) {
	let d = serde_cbor::to_vec(value).expect("bad ser");
	into.write_all(&d).await.expect("bad write");
	into.write_all(b"\n").await.expect("bad write");
}

/// # Panics
/// Panics if the struct sent by the other player is not a valid `NetMsg` or the connection is closed.
pub fn read_from<T: DeserializeOwned, R: Read>(from: &mut R) -> T {
	let d: Vec<u8> = from.bytes().flatten().take_while(|c| *c != b'\n').collect();
	serde_cbor::from_slice(&d).unwrap()
}

/// # Panics
/// Panics if the struct sent by the other player is not a valid `NetMsg` or the connection is closed.
pub async fn read_from_async<T: DeserializeOwned, R: AsyncRead + AsyncReadExt + Unpin>(
	from: &mut R,
) -> T {
	let mut d = vec![];
	loop {
		let b = from.read_u8().await.expect("bad read");
		if b == b'\n' {
			break;
		}
		d.push(b);
	}
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
