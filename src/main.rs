#![warn(clippy::pedantic)]
use netbattleship::flow::{GameFlow, GameFlowError};
use netbattleship::ui::flush;
use netbattleship::ui::parse_coord;
use netbattleship::Phase;
use std::io::stdin;
use std::net::SocketAddrV4;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
	/// The IP address of the other computer.
	pub server: SocketAddrV4,
	/// Whether to act as a server.
	#[structopt(short, long)]
	pub serve: bool,
}

#[tokio::main]
async fn main() {
	let args = Args::from_args();

	println!("Connecting...");
	let mut game = GameFlow::new(std::net::SocketAddr::V4(args.server), args.serve)
		.await
		.expect("Failed to connect");
	let mut stdin = stdin().lines().flatten();

	println!("Ready! Now, place your ships.");
	while let Phase::Placing(ship) = game.phase() {
		println!("{}", game.to_string());
		print!("Place the top-left section of your {:?} (like E5): ", ship);
		flush();
		let Some(pos) = parse_coord(&stdin.next().expect("Broken pipe")) else { println!("Those coordinates were malformed, try again."); continue };
		print!("Vertical (y)? ");
		flush();
		let v = stdin.next().expect("Broken pipe").starts_with('y');
		match game.place_ship(ship, pos, v) {
			Ok(()) => {}
			Err(GameFlowError::InvalidPlacement) => println!("Invalid placement, try again."),
			Err(e) => panic!("{}", e),
		}
	}

	println!("Ready to play! Choose your first target.");
	while matches!(game.phase(), Phase::Playing) {
		if game.my_turn() {
			println!("{}", game.to_string());
			print!("Choose your target (like E5): ");
			flush();
			let Some(aim) = parse_coord(&stdin.next().expect("Broken pipe")) else { println!("Those coordinates were malformed, try again."); continue };
			println!("Fire!!!");
			let result = game.fire(aim).await.expect("Running fire code failed.");
			if result.hit.is_some() {
				println!("KABOOM!");
			} else {
				println!("Splash...");
			}
			if let Some(ship) = result.sunk {
				println!("You sunk the enemy's {:?}.", ship);
			}
			if result.won {
				println!("You win!!!");
				break;
			}
		} else {
			println!("Waiting for your enemy to aim...");
			flush();
			let result = game.receive().await.expect("Couldn't receive fire.");
			if let Some(ship) = result.hit {
				println!("KABOOM! The enemy hit your {:?}!", ship);
			} else {
				println!("Splash...");
			}
			if let Some(ship) = result.sunk {
				println!("The enemy sunk your {:?}...", ship);
			}
			if result.won {
				println!("You lose...");
				break;
			}
		}
	}
}
