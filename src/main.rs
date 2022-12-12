#![warn(clippy::pedantic)]
use netbattleship::net;
use netbattleship::{net::read_from, net::write_to, net::Msg, ship::Ship, ui::parse_coord, Game};
use std::io::{stdout, BufRead, Write};
use std::{
	net::{SocketAddrV4, TcpListener, TcpStream},
	time::Duration,
};
use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
	/// The IP address of the other computer.
	pub server: SocketAddrV4,
	/// Whether to act as a server.
	#[structopt(short, long)]
	pub serve: bool,
}

fn main() {
	let args = Args::from_args();

	let mut stream = handshake(&args);
	let mut game = Game {
		you: !args.serve,
		..Default::default()
	};
	let mut stdin = std::io::stdin().lock().lines().flatten();

	println!("Ship placement phase...");
	ship_placement(&mut stdin, &mut game, &mut stream);

	println!("Done placing ships! Waiting for the other player...");
	assert_eq!(Msg::Finished, read_from(&mut stream));
	println!("Other player is finished! Starting the game...");
	game_loop(game, stdin, &mut stream);
	stream
		.shutdown(std::net::Shutdown::Both)
		.expect("Bad shutdown");
}

fn handshake(args: &Args) -> TcpStream {
	let mut stream = if args.serve {
		println!("Listening on {}", args.server);
		let list = TcpListener::bind(args.server).expect("Failed to listen.");
		list.accept().expect("Failed to receive connection.").0
	} else {
		println!("Connecting to {}", args.server);
		TcpStream::connect_timeout(
			&std::net::SocketAddr::V4(args.server),
			Duration::from_secs(10),
		)
		.expect("Failed to connect.")
	};
	println!("Got connection...");
	let msg = Msg::Hello(0);
	write_to(&msg, &mut stream);
	let resp: Msg = read_from(&mut stream);
	assert_eq!(msg, resp);
	println!("Got good handshake!");
	stream
}

fn ship_placement(
	stdin: &mut std::iter::Flatten<std::io::Lines<std::io::StdinLock>>,
	game: &mut Game,
	stream: &mut TcpStream,
) {
	for ship_kind in Ship::into_iter() {
		loop {
			println!("{}", String::from(game.clone()));
			print!("Input the position of {:?} (like E5): ", &ship_kind);
			flush();
			let input = stdin.next().expect("Broken pipe");
			if let Some((x, y)) = parse_coord(&input) {
				print!("Vertical? (yn): ");
				flush();
				let input = stdin.next().expect("Broken pipe");
				let vertical = input == "y";
				if ship_kind.place(&mut game.board[usize::from(game.you)], (x, y), vertical) {
					break;
				}
				println!("Bad placement, try again.");
			}
		}
	}
	net::write_to(&Msg::Finished, stream);
}

fn game_loop(
	mut game: Game,
	mut stdin: std::iter::Flatten<std::io::Lines<std::io::StdinLock>>,
	stream: &mut TcpStream,
) {
	loop {
		println!("{}", String::from(game.clone()));
		if game.turn == game.you {
			// Choose a target
			println!("It's your turn!");
			stdout().lock().write_all(&[7]).expect("Broken pipe");
			print!("Input a guess (like E5): ");
			flush();
			let coords = match parse_coord(&stdin.next().expect("Broken pipe")) {
				Some(c) => c,
				None => continue,
			};
			// Fire
			net::write_to(&Msg::Fire(coords.0, coords.1), stream);
			// Did we hit?
			let resp = net::read_from(stream);
			match resp {
				Msg::DidHit(false) => {
					println!("Miss...");
					game.board[1 ^ usize::from(game.you)]
						.board
						.insert(coords, Ship::Miss);
				}
				Msg::DidHit(true) => {
					println!("Hit!");
					game.board[1 ^ usize::from(game.you)]
						.board
						.insert(coords, Ship::Hit);
				}
				m => panic!("Unexpected {:?}", m),
			}
			// Did we sink?
			let resp = net::read_from(stream);
			match resp {
				Msg::Sunk(Ship::None) => {}
				Msg::Sunk(s) => {
					println!("Sunk {:?}!", s);
				}
				m => panic!("Unexpected {:?}", m),
			}
			// Is the game over?
			let resp = net::read_from(stream);
			match resp {
				Msg::Finished => {
					println!("You win!!!");
					break;
				}
				Msg::NotFinished => {}
				m => panic!("Unexpected {:?}", m),
			}
		} else {
			// Enemy's turn.
			println!("Waiting for the enemy to aim...");
			let msg = net::read_from(stream);
			let aim = match msg {
				Msg::Fire(x, y) => (x, y),
				m => panic!("Unexpected {:?}", m),
			};
			println!("The enemy fired at {}{}!", (aim.1 + b'A') as char, aim.0);
			// Did the enemy hit the ship?
			let hit = game.board[usize::from(game.you)]
				.board
				.insert(aim, Ship::Hit)
				.filter(|v| !matches!(v, Ship::Hit | Ship::None | Ship::Miss))
				.unwrap_or(Ship::None);
			match hit {
				Ship::None => {
					net::write_to(&Msg::DidHit(false), stream);
					println!("The enemy missed.");
				}
				s => {
					net::write_to(&Msg::DidHit(true), stream);
					println!("The enemy hit your {:?}!", s);
				}
			}
			// Did the enemy sink our ship?
			if !matches!(hit, Ship::None) && !game.board[usize::from(game.you)].contains(hit) {
				println!("The enemy has sank your {:?}!", hit);
				net::write_to(&Msg::Sunk(hit), stream);
			} else {
				net::write_to(&Msg::Sunk(Ship::None), stream);
			}
			// Did we lose?
			if game.board[usize::from(game.you)]
				.board
				.iter()
				.all(|(_, ship)| ship.is_empty())
			{
				println!("You lose!");
				net::write_to(&Msg::Finished, stream);
				break;
			}
			net::write_to(&Msg::NotFinished, stream);
		}
		game.turn ^= true;
	}
}

fn flush() {
	stdout().lock().flush().expect("Broken pipe");
}
