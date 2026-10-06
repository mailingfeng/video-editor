use std::{env,fs,io::{self,Write},thread,time::Duration};
fn main() {
 let args:Vec<String>=env::args().collect();
 fs::write(&args[2],std::process::id().to_string()).unwrap();
 match args[1].as_str() {
  "q_hang" => {let mut line=String::new();io::stdin().read_line(&mut line).unwrap();fs::write(&args[3],line).unwrap();loop{thread::sleep(Duration::from_millis(100));}},
  "closed" => {
   #[cfg(unix)] {unsafe extern "C" {fn close(fd:i32)->i32;} unsafe{close(1);close(2);}}
   #[cfg(windows)] {unsafe extern "C" {fn _close(fd:i32)->i32;} unsafe{_close(1);_close(2);}}
   thread::sleep(Duration::from_millis(200));
  },
  "chunked" => {let b="frame=3\n中文完成\n".as_bytes(); for c in b.chunks(2) {io::stdout().write_all(c).unwrap();io::stdout().flush().unwrap();}},
  "flood" => {for _ in 0..2000 {eprintln!("{}", "x".repeat(100));} println!("finished");},
  "hang" => {loop {thread::sleep(Duration::from_millis(100));}},
  _ => {std::process::exit(9);}
 }
}
