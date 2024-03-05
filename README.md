# Axum error handling like Pavex

This is a simple PoC for how to do error handling, and in particular tracing, like in Pavex.

There are still some ergonomics to work on, and I guess it could be prettier, but I guess it works.

## Getting started

To see it in action, just clone the repo and run `cargo run` and call localhost:8888

You should now see error logs in the terminal with the right errors _before_ they have been called IntoResponse.
