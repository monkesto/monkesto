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

## Install postgres, and create a database for the server:
```
sudo -iu postgres psql
```

## Inside the postgres terminal:
```
CREATE ROLE username WITH LOGIN PASSWORD 'password';

CREATE DATABASE dbname WITH OWNER username;

\q
```

## Clone the repo:
```
git clone https://github.com/shaggysa/monkesto.git
cd monkesto
```

## Create a .env file with postgres credentials:
```
touch .env

echo "POSTGRES_USER=username" >> .env
echo "POSTGRES_PASSWORD=password" >> .env
echo "DATABASE_HOST=localhost" >> .env
echo "POSTGRES_DB=dbname" >> .env
```

## Start the server:
```
cargo leptos watch 
```

## If you do not have postgres installed already:

### macos:
```
brew install postgresql@15
```

### debian:
```
sudo apt install postgresql
```

### fedora:
```
sudo dnf install postgresql-server postgresql
```

### arch:
```
sudo pacman -S postgresql
```


## If you do not have cargo-leptos already:
```
cargo install --locked cargo-leptos
```
