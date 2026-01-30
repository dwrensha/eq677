use std::fs;
use std::path::Path;

// Include the crate for db access
use eq677::*;

fn main() {
    let out_dir = Path::new("visualizations");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir).expect("Failed to create output directory");
    }

    for (name, m) in db() {
        let filename = out_dir.join(format!("{}_{}.png", name.0, name.1));
        let png_data = magma_to_png(&m);
        fs::write(&filename, png_data).expect("Failed to write PNG");
        println!("Wrote {}", filename.display());
    }
}

fn magma_to_png(m: &MatrixMagma) -> Vec<u8> {
    let n = m.n;
    if n == 0 {
        return minimal_png();
    }

    // Build raw image data: each row starts with filter byte (0 = none), then RGB pixels
    let mut raw_data = Vec::with_capacity(n * (1 + n * 3));
    for x in 0..n {
        raw_data.push(0); // filter byte: none
        for y in 0..n {
            let val = m.f(x, y);
            let (r, g, b) = value_to_rgb(val, n);
            raw_data.push(r);
            raw_data.push(g);
            raw_data.push(b);
        }
    }

    // Compress with zlib (uncompressed deflate)
    let compressed = zlib_compress_uncompressed(&raw_data);

    // Build PNG
    build_png(n as u32, n as u32, &compressed)
}

fn value_to_rgb(val: usize, n: usize) -> (u8, u8, u8) {
    if val == usize::MAX || n == 0 {
        return (128, 128, 128); // gray for undefined
    }

    // Smooth cyclic rainbow using HSL with S=1, L=0.5 (full saturation, medium lightness)
    // Hue cycles from 0 to 1 as val goes from 0 to n-1
    let hue = (val as f64) / (n as f64);
    hsl_to_rgb(hue, 1.0, 0.5)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h * 6.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;

    (r, g, b)
}

fn build_png(width: u32, height: u32, compressed_data: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();

    // PNG signature
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8);  // bit depth
    ihdr.push(2);  // color type: RGB
    ihdr.push(0);  // compression method
    ihdr.push(0);  // filter method
    ihdr.push(0);  // interlace method
    write_chunk(&mut png, b"IHDR", &ihdr);

    // IDAT chunk
    write_chunk(&mut png, b"IDAT", compressed_data);

    // IEND chunk
    write_chunk(&mut png, b"IEND", &[]);

    png
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    // Length (4 bytes, big-endian)
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    // Chunk type
    out.extend_from_slice(chunk_type);
    // Data
    out.extend_from_slice(data);
    // CRC32 of chunk type + data
    let crc = crc32(&[chunk_type.as_slice(), data].concat());
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    // CRC32 with PNG polynomial
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn zlib_compress_uncompressed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();

    // Zlib header: CMF=0x78 (deflate, 32K window), FLG=0x01 (no dict, check bits)
    out.push(0x78);
    out.push(0x01);

    // Deflate stream with uncompressed blocks
    // Each block can hold up to 65535 bytes
    let mut remaining = data;
    while !remaining.is_empty() {
        let chunk_size = remaining.len().min(65535);
        let is_final = chunk_size == remaining.len();

        // Block header: BFINAL (1 bit) + BTYPE=00 (2 bits) = stored block
        // For stored blocks, we need to pad to byte boundary, but since we're at byte start, just write the byte
        out.push(if is_final { 0x01 } else { 0x00 });

        // LEN and NLEN (little-endian)
        let len = chunk_size as u16;
        let nlen = !len;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());

        // Data
        out.extend_from_slice(&remaining[..chunk_size]);
        remaining = &remaining[chunk_size..];
    }

    // Adler-32 checksum of uncompressed data
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());

    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    const MOD_ADLER: u32 = 65521;

    for &byte in data {
        a = (a + byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }

    (b << 16) | a
}

fn minimal_png() -> Vec<u8> {
    // 1x1 black pixel PNG
    build_png(1, 1, &zlib_compress_uncompressed(&[0, 0, 0, 0]))
}
