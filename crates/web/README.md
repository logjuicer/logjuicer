# logreduce-web

Develop the web interface by first generating a report with:
`cargo run -p logreduce-cli -- --report report.bin ...`. Then
run the

```ShellSession
trunk serve ./dev.html --address 0.0.0.0
```
