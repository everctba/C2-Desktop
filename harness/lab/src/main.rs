use std::env;
use std::path::PathBuf;

mod render;
mod store;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("uso: taskline add <titulo> | list | done <id>");
        std::process::exit(2);
    }
    let path = PathBuf::from(".taskline.json");
    let mut dados = store::Store::load(&path);
    match args[0].as_str() {
        "add" => {
            let titulo = args[1..].join(" ");
            match dados.add(&titulo) {
                Ok(id) => {
                    let _ = dados.save(&path);
                    println!("{id}");
                }
                Err(erro) => {
                    eprintln!("{erro}");
                    std::process::exit(1);
                }
            }
        }
        "list" => {
            for tarefa in dados.list() {
                let marca = if tarefa.done { "x" } else { " " };
                println!("[{marca}] {} {}", tarefa.id, tarefa.title);
            }
        }
        "done" => {
            let Ok(id) = args[1].parse::<u64>() else {
                eprintln!("id invalido");
                std::process::exit(2);
            };
            if dados.complete(id) {
                let _ = dados.save(&path);
                println!("ok");
            } else {
                eprintln!("tarefa nao encontrada");
                std::process::exit(1);
            }
        }
        "html" => {
            println!("{}", render::pagina(&dados.list()));
        }
        outro => {
            eprintln!("comando desconhecido: {outro}");
            std::process::exit(2);
        }
    }
}
