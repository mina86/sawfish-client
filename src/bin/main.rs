// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>
//
// setroot is free software: you can redistribute it and/or modify it under the
// terms of the GNU Lesser General Public License as published by the Free
// Software Foundation; either version 3 of the License, or (at your option) any
// later version.
//
// setroot is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
// A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// sawfish-client.  If not, see <http://www.gnu.org/licenses/>.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Example program using the sawfish-client library.
///
/// ```shell
/// $ cargo run -- '(system-name)'
/// > (system-name)
/// < "darkstar.example.net"
/// ```
fn main() -> std::process::ExitCode {
    let mut args = std::env::args_os();
    let argv0 = PathBuf::from(args.next().unwrap());
    let argv0 = argv0.display();

    // Establish connection.  open will read $DISPLAY to get the display name.
    let mut conn = match sawfish_client::open(None) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("{argv0}: {err}");
            return std::process::ExitCode::FAILURE;
        }
    };

    // Sends a single form for evaluation.  If `is_async` is true, does not read
    // the response.
    let mut eval = |form: &[u8], is_async: bool| {
        println!("> {}", String::from_utf8_lossy(form));
        let res = if is_async {
            conn.eval_async(form)
        } else {
            conn.eval(form).map(|res| {
                let (ch, data) = match res {
                    Ok(data) => ('<', data),
                    Err(data) => ('!', data),
                };
                println!("{ch} {}", String::from_utf8_lossy(&data));
            })
        };
        if let Err(err) = res {
            eprintln!("{argv0}: {err}");
        }
    };

    // Process arguments.
    let mut found = false;
    let mut quiet = false;
    let mut dash_dash = false;
    while let Some(arg) = args.next() {
        if dash_dash || !arg.as_encoded_bytes().starts_with(b"-") {
            found = true;
            eval(arg.as_encoded_bytes(), quiet);
        } else if arg == "-h" || arg == "--help" {
            found = false;
            break;
        } else if arg == "-q" || arg == "--quiet" {
            quiet = true;
        } else if arg == "-Q" || arg == "--no-quiet" {
            quiet = false;
        } else if arg == "-" || arg == "--stdin" {
            found = true;
            let mut form = Vec::new();
            match std::io::stdin().read_to_end(&mut form) {
                Ok(0) => continue,
                Ok(_) => eval(form.as_slice(), quiet),
                Err(err) => eprintln!("{argv0}: {err}"),
            }
        } else if let Some(func) = is_func_arg(&arg) {
            found = true;
            if let Some(form) = build_form(func, args) {
                eval(&form, quiet);
                break;
            } else {
                eprintln!("{argv0}: -f requires an argument");
                return std::process::ExitCode::FAILURE;
            }
        } else if arg == "--" {
            dash_dash = true;
        } else {
            eprintln!(
                "{argv0}: unknown argument: {}",
                Path::new(arg.as_os_str()).display()
            );
            return std::process::ExitCode::FAILURE;
        }
    }

    // If no forms were given as arguments, print help screen.
    if !found {
        println!(
            "usage: {argv0} (-q | -Q | <form> | -)… [-f <func> <arg>…]
Options:
  -q --quiet      Don’t wait for server response after sending a form.
  -Q --no-quiet   Wait for a response after sending a form.
  -  --stdin      Read form from standard input until EOF.
  -f --func       Send `(<func> <arg>…)` form for evaluation.
  <form>          Send `<form>` for evaluation."
        )
    }
    std::process::ExitCode::SUCCESS
}


/// Checks whether argument is `-f`/`--func` and if so, whether `<func>` is
/// attached to it, as in `-fsystem-name` or `--func=system-name`.
fn is_func_arg(arg: &OsStr) -> Option<Option<&OsStr>> {
    if arg == "-f" || arg == "--func" {
        Some(None)
    } else {
        let arg = arg.as_encoded_bytes();
        arg.strip_prefix(b"-f").or_else(|| arg.strip_prefix(b"--func=")).map(
            |func| {
                // SAFETY We’ve stripped ASCII string from the front which keeps
                // the arg a valid OsStr.
                Some(unsafe { OsStr::from_encoded_bytes_unchecked(func) })
            },
        )
    }
}

/// Constructs form from the `-f`/`--func` argument and rest of the arguments.
///
/// `func` is the inner-value returned by `is_func_arg`.  Returns `None` if
/// resulting form is empty, i.e. there are no arguments following `-f`/`--func`
/// switch.
fn build_form(func: Option<&OsStr>, args: std::env::ArgsOs) -> Option<Vec<u8>> {
    let mut form = Vec::new();
    if let Some(func) = func {
        form.push(b'(');
        form.extend_from_slice(func.as_encoded_bytes());
    }
    for arg in args {
        form.push(b' ');
        form.extend_from_slice(arg.as_encoded_bytes());
    }
    form.push(b')');
    form[0] = b'(';
    (form.len() > 2).then_some(form)
}
