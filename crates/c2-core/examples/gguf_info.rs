//! Utilitario de desenvolvimento: imprime o cabecalho de um GGUF.
//!
//!   cargo run -p c2-core --example gguf_info -- "H:\modelos\modelo.gguf"

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("uso: cargo run -p c2-core --example gguf_info -- <arquivo.gguf>");
        std::process::exit(2);
    };

    match c2_core::gguf::read_info(std::path::Path::new(&path)) {
        Ok(info) => {
            println!("{}", serde_json::to_string_pretty(&info).unwrap());
        }
        Err(err) => {
            eprintln!("erro: {err}");
            std::process::exit(1);
        }
    }
}
