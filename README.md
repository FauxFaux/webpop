A "live port" of libwebp to rust.

A pinned commit of libwebp exists in upstream/.

Initially focused on simple encoding, then complex encoding, then decoding.

See the [lossless encoder porting plan](docs/LOSSLESS_PORT_PLAN.md) for the
`WebPEncodeLosslessRGBA` dependency map, implementation order, and testing strategy.

