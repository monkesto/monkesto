# Official site:
You can connect to [staging.monkesto.com] to try out the latest version.
It is updated with every commit to the main branch. Be aware that backwards compatibility between updates is not currently guaranteed,
and breaking changes may cause the website to be reset at any time. Any lost data will not be recovered.

[staging.monkesto.com]: https://staging.monkesto.com

# To run the server yourself:

## Create a .env file with postgres credentials:
```
touch .env

echo "POSTGRES_USER=username" >> .env
echo "POSTGRES_PASSWORD=password" >> .env
echo "POSTGRES_DB=dbname" >> .env
```

## Start the server container (requires docker):
```
curl -sSL -o docker-compose.deploy.yml https://raw.githubusercontent.com/shaggysa/monkesto/main/docker-compose.deploy.yml

docker compose -f docker-compose.deploy.yml up --pull always
```

## Or, you can use the latest pre-release image (created at every commit to main):
```
curl -sSL -o docker-compose.prerelease.yml https://raw.githubusercontent.com/shaggysa/leptos-prototyping/main/docker-compose.prerelease.yml

docker compose -f docker-compose.prerelease.yml up --pull always
```

### Builds are currently unstable, and database resets will almost certainly be necessary at some point. You can do this with:
```
docker compose -f docker-compose.deploy.yml down -v
```

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
git clone https://github.com/shaggysa/monkesto.git
cd monkesto
```

## Create a .env file with postgres credentials:
```
touch .env

echo "POSTGRES_USER=monkesto" >> .env
echo "POSTGRES_PASSWORD=monkesto" >> .env
echo "DATABASE_HOST=localhost" >> .env
echo "POSTGRES_DB=monkesto" >> .env
```

## Start the server:
```
cargo leptos watch 
```

## If you do not have cargo-leptos already:
```
cargo install --locked cargo-leptos
```
