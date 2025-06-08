# Myco

![logo](./docs/logo-small.png)

Myco is a new, experimental JavaScript runtime that implements the
[object-capability model](https://en.wikipedia.org/wiki/Object-capability_model), built in Rust on top of V8. It's currently highly experimental,
intended to explore new territory in server-side application security.

## Motivation

When Log4Shell exploded in the Java world in 2021, I was immediately struck by how preventable the bug
should have been. A logging library which most people were using to write to log files was  authorized
to make network calls! How did this go unnoticed?

Over the years since, I've spent a lot of time exploring different solutions the library security
problem. Myco brings the object-capability model to JavaScript to try to solve the dependency crisis.

## Getting started

Myco is currently in a very early stage of development. It's not ready for use yet, but if you want
to play around with it, you can install it:

```shell
cargo install --path cli
```

To create a new myco project:

```shell
myco init my_project && cd my_project
```

To type check it:

```shell
myco run check
```

And to run it:

```shell
myco run
```

## Structure of a Myco project

The initial Myco project has the following directory structure:

```
my_project
├── src
│   └── index.ts
├── vendor
│   └── @myco
│       └── check
│       └── core
│       └── typescript
├── .gitignore
├── myco.toml
└── tsconfig.json
```

The `vendor` folder is .gitignored by default: it's created and maintained by Myco
and contains your `deps` defined in `myco.toml`.

## Structure of the Myco codebase

The project is split between Rust code (found in [src](./src)) and a JavaScript runtime
written in TypeScript (found in [runtime](./runtime)). The output is a single binary that
includes a snapshot of the runtime. The binary is created as
[target/release/myco](./target/release/myco) or [target/debug/myco](./target/debug/myco).
