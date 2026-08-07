# Official site:

You can connect to [monkesto.com] to try out the latest version.
It is updated with every commit to the main branch. Any user/journal data may be reset at any commit.

[monkesto.com]: https://monkesto.com

# Or build from source:

## Clone the repo:

```
git clone https://github.com/monkesto/monkesto.git
cd monkesto
```

## install build dependencies:

```
cargo install cargo-watch
cargo install cargo-make
cargo install sqlx-cli
npm install
```

### linker

Monkesto uses the [mold](https://github.com/rui314/mold) linker on linux. It must be installed and in your $PATH for the
build to succeed.

## Configure the database:

Monkesto requires PostgreSQL at runtime. Set its connection URL and enable
SQLx's offline mode in `.env` so the project can compile against the checked-in
`.sqlx` query metadata before a new database has been initialized:

```dotenv
DATABASE_URL=postgres://monkesto:monkesto@localhost:5432/monkesto
SQLX_OFFLINE=true
```

After changing a `sqlx::query!` invocation or its database schema, refresh the
checked-in query metadata against a running, up-to-date database:

```sh
cargo sqlx prepare -- --all-targets
```

## Start the server:

```
cargo make
```
