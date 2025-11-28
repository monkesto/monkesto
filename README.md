# To run the server:

## Create a .env file with postgres credentials:
```
touch .env

echo "POSTGRES_USER=username" >> .env
echo "POSTGRES_PASSWORD=password" >> .env
echo "POSTGRES_DB=dbname" >> .env
```

## Start the server container (requires docker):
```
curl -sSL -o docker-compose.deploy.yml https://raw.githubusercontent.com/shaggysa/leptos-prototyping/main/docker-compose.deploy.yml

docker compose -f docker-compose.deploy.yml up
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

echo "POSTGRES_USER=username" >> .env
echo "POSTGRES_PASSWORD=password" >> .env
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
