# dacha

This is a monorepo/ecosystem of software/hardware solutions built by [Dennis](https://github.com/dennisss). Code is written primarily in Rust. Check out the partial list of what's available below.

## Components

### Systems

- [Cluster Orchestration](./pkg/container/index.md) : Kubernetes/Borg like containerization and service mesh solution.
- [Metadata Storage](./pkg/datastore/src/meta/index.md) : Chubby/Etcd like key-value store.
- [Builder](./pkg/builder/index.md) : A Bazel/Buck like dependency graph based builded system.

### Libraries

- [Executor](./pkg/executor/index.md): Runtime for executing async futures.
- [CLI Arguments Parser](./pkg/base/args/index.md)
- [Cryptography](./pkg/crypto/index.md): Suite of most common encryption/hashing/randomness
  algorithms. Also includes support for TLS and X.509.
- [HTTP](./pkg/http/index.md): HTTP 1/2 client/server implementation.
- [RPC](./pkg/rpc/index.md): gRPC compatible remote procedure call framework.
- [Math](./pkg/math/index.md): Linear algebra, optimization, geometric algorithms, big integers, etc.
- [Compression](./pkg/compression/index.md)
- [Linux Syscall Bindings](./pkg/sys/)
- [USB Device/Host Driver](./pkg/usb/index.md)
- [JSON](./pkg/json/)
- [Image Encoding/Decoding](./pkg/image/)
- [Raft](./pkg/raft/README.md) : Implementation of Raft consensus and everything need to make a replicated state machine from an existing non-replicated one.
- [Embedded DB](./pkg/sstable/index.md) : A LSM style single process database compatible with RocksDB/LevelDB.
- [Graphics](./pkg/graphics/) : A UI and rendering framework.


### Compilers

- [Automata / Regular Expressions](./pkg/automata/index.md)
- [Protobuf](./pkg/protobuf/index.md): Support for serialization/deserialization of Protocol Buffers either via code generation or dynamic reflection.
- [ASN.1](./pkg/asn/index.md): Compiler for safe accessors and serialization/deserialization of ASN.1 messages.
- [Markdown](./pkg/markdown/index.md)
- [Skylark](./pkg/skylark/index.md): Python like evaluation.

## Directory Structure

Directories under this repository are used as follows:

- `pkg/[name](/[subname])*` : First party Rust crates
- `doc/` : General documentation not associated with any specific package
- `testdata/` : Data for testing stuff under `pkg/`
- `third_party/` : Code and data dependencies imported from external sources.
    - As a general rule of thumb, we'd prefer to clone any dependencies into here rather than relying on package manager based vendoring.

