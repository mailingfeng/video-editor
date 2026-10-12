use std::{env,fs,io::{self,Write},thread,time::Duration};
fn main() {
 let args:Vec<String>=env::args().collect();
 fs::write(&args[2],std::process::id().to_string()).unwrap();
 match args[1].as_str() {
  "filters" | "filters_loaded" | "filters_hold" => {
   let mut report=String::new();
   for arg in &args[4..] {report.push_str(&format!("argument:{arg}\n"));}
   for (flag, label) in [("-/filter:v", "video"),("-/filter:a", "audio")] {
    if let Some(at)=args.iter().position(|arg| arg==flag) {
     let file=&args[at+1];
     report.push_str(&format!("loaded:{label}:{}\n",fs::read_to_string(file).unwrap()));
     #[cfg(unix)] {
      use std::os::unix::fs::PermissionsExt;
      report.push_str(&format!("mode:{}\n",fs::metadata(file).unwrap().permissions().mode()&0o777));
     }
    }
   }
   fs::write(&args[3],&report).unwrap();
   print!("{report}");io::stdout().flush().unwrap();
   if args[1]!="filters" {
    if args[1]=="filters_loaded" {println!("frame=1\nprogress=continue");io::stdout().flush().unwrap();}
    let mut line=String::new();io::stdin().read_line(&mut line).unwrap();
   }
  },
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
