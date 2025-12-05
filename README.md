# Official site:
You can connect to [staging.monkesto.com] to try out the latest version.
It is updated with every commit to the main branch. Be aware that backwards compatibility between updates is not currently guaranteed,
and breaking changes may cause the website to be reset at any time. Any lost data will not be recovered.

[staging.monkesto.com]: https://staging.monkesto.com

# To run the server yourself:

## Download the default docker-compose file provided in the repo:
```
curl https://raw.githubusercontent.com/shaggysa/leptos-prototyping/main/docker-compose.yml -o docker-compose-yml
```

## Alternatively, use the builder script to create a custom docker compose file:
This script supports many options, such as providing your own postgres database 
or setting options for the inbuilt database.

```
curl https://raw.githubusercontent.com/shaggysa/leptos-prototyping/main/docker_compose_builder.py | python3
```


## Deploy your docker compose file:
```
docker compose up --pull always
```
Note that your docker compose file also serves as a .env file, so keep it secure.

### Builds are currently unstable, and database resets will almost certainly be necessary at some point. You can do this with:
```
docker compose down -v
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
git clone https://github.com/shaggysa/leptos-prototyping.git
cd leptos-prototyping
```

## Create a .env file with postgres credentials:
```
touch .env

echo "postgres://username:password@localhost:5432/dbname"
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
