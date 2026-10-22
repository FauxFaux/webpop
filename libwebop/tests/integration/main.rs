use wasmtime::{Engine, Linker, Module, Store};

const ENCODER: &[u8] = include_bytes!("webp-encode-lossless.wasm");

#[test]
fn lossless_rgba_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::default();
    let module = Module::new(&engine, ENCODER)?;
    let mut store = Store::new(&engine, ());
    let mut linker = Linker::new(&engine);
    linker.func_wrap("env", "emscripten_notify_memory_growth", |_: i32| {})?;
    let instance = linker.instantiate(&mut store, &module)?;

    // The module was built with STACK_OVERFLOW_CHECK=2. Initialize the C-stack
    // bounds before running its constructors or calling the encoder.
    instance
        .get_typed_func::<(), ()>(&mut store, "emscripten_stack_init")?
        .call(&mut store, ())?;
    let base = instance
        .get_typed_func::<(), u32>(&mut store, "emscripten_stack_get_base")?
        .call(&mut store, ())?;
    let end = instance
        .get_typed_func::<(), u32>(&mut store, "emscripten_stack_get_end")?
        .call(&mut store, ())?;
    instance
        .get_typed_func::<(u32, u32), ()>(&mut store, "__set_stack_limits")?
        .call(&mut store, (base, end))?;
    instance
        .get_typed_func::<(), ()>(&mut store, "_initialize")?
        .call(&mut store, ())?;

    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let malloc = instance.get_typed_func::<u32, u32>(&mut store, "malloc")?;
    let free = instance.get_typed_func::<u32, ()>(&mut store, "free")?;
    let webp_free = instance.get_typed_func::<u32, ()>(&mut store, "WebPFree")?;
    let encode = instance
        .get_typed_func::<(u32, u32, u32, u32, u32), u32>(&mut store, "WebPEncodeLosslessRGBA")?;

    let (width, height) = (17_u32, 13_u32);
    // Odd dimensions, varied colours, and nonzero alpha exercise exact RGBA
    // preservation. libwebp may discard RGB under fully transparent pixels.
    let rgba: Vec<u8> = (0..width * height)
        .flat_map(|i| [i as u8, (i * 37) as u8, (i * 91) as u8, (i % 255 + 1) as u8])
        .collect();
    let input = malloc.call(&mut store, rgba.len() as u32)?;
    let output = malloc.call(&mut store, 4)?;
    assert_ne!(input, 0);
    assert_ne!(output, 0);
    memory.write(&mut store, input as usize, &rgba)?;
    memory.write(&mut store, output as usize, &[0; 4])?;
    let size = encode.call(&mut store, (input, width, height, width * 4, output))?;
    assert_ne!(size, 0, "lossless encoding failed");
    let mut pointer = [0; 4];
    memory.read(&store, output as usize, &mut pointer)?;
    let encoded = u32::from_le_bytes(pointer);
    assert_ne!(encoded, 0);
    let mut bytes = vec![0; size as usize];
    memory.read(&store, encoded as usize, &mut bytes)?;
    webp_free.call(&mut store, encoded)?;
    free.call(&mut store, output)?;
    free.call(&mut store, input)?;
    drop(store);

    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    assert_eq!(&bytes[12..16], b"VP8L");
    let decoded =
        image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)?.into_rgba8();
    assert_eq!(decoded.dimensions(), (width, height));
    assert_eq!(decoded.as_raw(), &rgba);
    Ok(())
}
