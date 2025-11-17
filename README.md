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

The crate defines one Cargo feature: `expemirental-xcb`.  It adds experimental
support for X11-based communication with Sawfish.  Normally, the library
connects to Sawfish via a Unix socket (located in `/tmp/.sawfish-$LOGNAME`
directory).  With this feature enabled, if connecting to the socket fails, the
library will try to use X11-based communication instead.  However, as per its
name, the feature is experimental and has not been extensively tested yet.
