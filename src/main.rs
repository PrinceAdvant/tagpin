use std::process::ExitCode;

fn main() -> ExitCode {
    let mut strict = false;
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-h" | "--help" => {
                eprintln!("usage: tagpin [--strict] Dockerfile compose.yml ...");
                eprintln!("Lists image references and exits 1 if any use latest or no tag.");
                eprintln!("--strict also fails on tags that are not pinned by digest.");
                return ExitCode::from(2);
            }
            "--strict" => strict = true,
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        eprintln!("usage: tagpin [--strict] Dockerfile compose.yml ...");
        return ExitCode::from(2);
    }
    let mut bad = 0;
    for f in &files {
        let text = match std::fs::read_to_string(f) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{f}: {e}");
                return ExitCode::from(2);
            }
        };
        for r in tagpin::scan(&text, f) {
            let fail = r.pin.is_floating() || (strict && r.pin == tagpin::Pin::Tag);
            if fail {
                bad += 1;
            }
            println!("{}:{:<5} {:<9} {}", r.file, r.line, r.pin.label(), r.image);
        }
    }
    if bad > 0 {
        eprintln!("{bad} unpinned image reference(s)");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
