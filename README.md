# fatrace-rs

`fatrace-rs` is a simple file access monitor for Linux systems using `fanotify(7)`, inspired by [`fatrace`](https://github.com/martinpitt/fatrace), originally developed by Martin Pitt.

---

## About

`fatrace-rs` logs file access events in real time using the `fanotify` subsystem.

The output format closely follows the original `fatrace` tool:
```
process_name(PID): EVENT /path/to/file
```

Examples of event codes:
- `O` — file opened
- `R` — file read
- `W` — file written
- `C` — file closed after writing
- `<`, `>` — file moved
- `+`, `D` — file created or deleted

---

## Resources

- [Original fatrace (C implementation)](https://github.com/martinpitt/fatrace)
- [Linux fanotify(7) manual](https://man7.org/linux/man-pages/man7/fanotify.7.html)
- [nix crate documentation](https://docs.rs/nix/)

---

## License

MIT (this project) — not affiliated with the original `fatrace`.
