# Examples

Demonstrates sending and recieving files

Sender example:

`cargo run --example sender`

Reciever example:

`cargo run --example reciever`

They will ask for each others ip addresses with ports.
If you are running both on a single computer you can do 127.0.0.1:\<port\>.

You can also use theese across the open internet, without port forwarding. You just replace the ip:s with the global ips. The ports stay the same. This is possible without port-forwards because of something called [udp-holepunching](https://en.wikipedia.org/wiki/UDP_hole_punching).
