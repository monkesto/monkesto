# Official site:

You can connect to [monkesto.com] to try out the latest version.
It is updated with every commit to the main branch. Be aware that backwards compatibility between updates is not currently guaranteed,
and breaking changes may cause the website to be reset at any time. Any lost data will not be recovered.

[monkesto.com]: https://monkesto.com

# Or build from source:

## Create a new docker database container:

```sh
docker create \
  -e POSTGRES_PASSWORD='monkesto' \
  -e POSTGRES_USER='monkesto' \
  -e POSTGRES_DB='monkesto' \
  --name monkesto-db \
  -p 5432:5432 \
  postgres
docker start monkesto-db
```

## Clone the repo:

```
git clone https://github.com/monkesto/monkesto.git
cd monkesto
```

## Create a .env file with postgres credentials:

```
touch .env

echo "DATABASE_URL=postgresql://monkesto:monkesto@localhost:5432/monkesto"
```

## Start the server:

```
cargo leptos watch
```

## If you do not have cargo-leptos already:

```
cargo install --locked cargo-leptos
```
