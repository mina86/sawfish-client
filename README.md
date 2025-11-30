# sawfish-client

A library for handling communication with the [Sawfish window manager][1] to
allow remote form evaluation.  In other words, Rust implementation of the
`sawfish-client` program shipped with Sawfish.

[1]: https://sawfish.tuxfamily.org/


## Example usage

```rust,no_run
fn sawfish_eval(form: &str) -> Result<Vec<u8>, ()> {
    // Establish connection.  open will read
    // $DISPLAY to get the display name.
    let mut conn = sawfish_client::open(None)
        .map_err(|err| { eprintln!("{err}"); })?;

    // Evaluate the form.
    println!(">>> {form}");
    match conn.eval(form) {
        Err(err) => {
            eprintln!("{err}");
            Err(())
        }
        Ok(Err(data)) => {
            let msg = String::from_utf8_lossy(&data);
            println!("!!! {msg}");
            Err(())
        }
        Ok(Ok(data)) => {
            let msg = String::from_utf8_lossy(&data);
            println!("<<< {msg}");
            Ok(data)
        }
    }
}
```

Furthermore, the crate comes with an example binary which can be examined to see
how the library functions.


## Features

The crate defines the following Cargo feature:

* `async` — adds `AsyncClient` type which uses `future_io` traits to support
  asynchronous I/O.  It can be used with any async runtime so long as
  a compatible async I/O object is provided.  Because opening the Unix socket
  depends on the runtime, with `AsyncClient` that now must be done by the
  caller.

* `tokio` — adds `TokioClient` type alias and `open_tokio` function which
  simplify using the library with the Tokio async runtime.  This feature does
  not introduce any new capabilities to `sawfish-client` but is provided for
  convenience of Tokio users.  This feature implies `async`.

* `expemirental-xcb` — adds experimental support for X11-based communication
  with Sawfish.  Normally, the library connects to Sawfish via a Unix socket.
  With this feature, if connecting to the socket fails, it tries to use
  X11-based communication instead.  Note that this feature is only supported
  with synchronous client.
