#!/usr/bin/env python3
"""Write a tiny but structurally valid GGUF v3 file.

Used to produce screenshots of the model library on a machine without a real
llama.cpp model. The file has a real header, real metadata, real tensor
descriptors and real (zeroed) F32 payload, so ATOM's own parser reads it
exactly like any other GGUF. It is a fixture, not a usable model.

Usage: scripts/make_demo_gguf.py <output.gguf>
"""

import struct
import sys

ALIGNMENT = 32

# GGUF metadata value types
U8, I8, U16, I16, U32, I32, F32, BOOL, STRING, ARRAY, U64, I64, F64 = range(13)
# GGML tensor types
GGML_F32 = 0


def s(value: str) -> bytes:
    raw = value.encode("utf-8")
    return struct.pack("<Q", len(raw)) + raw


def kv_str(key: str, value: str) -> bytes:
    return s(key) + struct.pack("<I", STRING) + s(value)


def kv_u32(key: str, value: int) -> bytes:
    return s(key) + struct.pack("<I", U32) + struct.pack("<I", value)


def kv_f32(key: str, value: float) -> bytes:
    return s(key) + struct.pack("<I", F32) + struct.pack("<f", value)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    out = sys.argv[1]

    metadata = [
        kv_str("general.architecture", "llama"),
        kv_str("general.name", "ATOM demo tiny"),
        kv_str("general.basename", "demo-tiny"),
        kv_str("general.quantization_version", "ATOM fixture"),
        kv_u32("general.file_type", 1),
        kv_u32("llama.block_count", 2),
        kv_u32("llama.context_length", 2048),
        kv_u32("llama.embedding_length", 64),
        kv_u32("llama.feed_forward_length", 128),
        kv_u32("llama.attention.head_count", 4),
        kv_u32("llama.attention.head_count_kv", 4),
        kv_f32("llama.attention.layer_norm_rms_epsilon", 1e-5),
        kv_str("tokenizer.ggml.model", "llama"),
    ]

    tensors = [
        ("token_embd.weight", [64, 128]),
        ("blk.0.attn_q.weight", [64, 64]),
        ("blk.1.ffn_down.weight", [128, 64]),
        ("output.weight", [64, 128]),
    ]

    header = struct.pack("<I", 0x46554747)  # "GGUF"
    header += struct.pack("<I", 3)
    header += struct.pack("<Q", len(tensors))
    header += struct.pack("<Q", len(metadata))
    body = b"".join(metadata)

    offset = 0
    infos = b""
    payload = b""
    for name, dims in tensors:
        count = 1
        for d in dims:
            count *= d
        infos += s(name) + struct.pack("<I", len(dims))
        infos += b"".join(struct.pack("<Q", d) for d in dims)
        infos += struct.pack("<I", GGML_F32)
        infos += struct.pack("<Q", offset)
        offset += count * 4
        payload += b"\x00" * (count * 4)

    blob = header + body + infos
    pad = (-len(blob)) % ALIGNMENT
    blob += b"\x00" * pad

    with open(out, "wb") as handle:
        handle.write(blob)
        handle.write(payload)
    print(f"wrote {out}: {len(blob) + len(payload)} bytes, {len(tensors)} tensors")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
