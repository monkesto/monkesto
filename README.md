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

`mdbtools` and `sqlite3` are required for importing Jewel databases

### linker

Monkesto uses the [mold](https://github.com/rui314/mold) linker on linux. It must be installed and in your $PATH for the
build to succeed.

## Configure the environment:

### postgres
Monkesto requires PostgreSQL at build and runtime. Run db_setup.sql against your database to allow to prepare the schema expected by the sqlx query macros.

```dotenv
DATABASE_URL=postgres://monkesto:monkesto@localhost:5432/monkesto
```

### base url
Webauthn requires the base url of the deployed site. This is defined with the `RAILWAY_PULBIC_DOMAIN` environment arg. 

If it is not present, `localhost:3000` will be assumed. 

If the base url is incorrect, passkey creation and login will not work.

### email
Monkesto uses [Resend](https://resend.com/) for email verification and sending authentication codes.

- `RESEND_EMAIL` is the address to send the email from ("Monkesto <noreply@monkesto.com>", for example).
- `RESEND_API_KEY` is the api key used to send said emails.

If either of these variables are missing, email verification will be skipped and account recovery via email will be unavailable. 

### object storage
Monkesto uses the S3 api to store uploaded files such as images. 
The following variables are needed from an S3-compatible service:

- `S3_ENDPOINT`
- `S3_ACCESS_KEY_ID`
- `S3_SECRET_ACCESS_KEY`

If any of these variables are missing, file storage and retrevial will be unavailable.

## Start the server:

```
cargo make
```

## pre-commit hooks
`scripts/pre-commit.sh` contains several important steps that correspond with the github CI:

- `cargo fmt` will ensure that the code is properly formatted
- `cargo sqlx prepare && git add .sqlx` ensures that the sqlx query cache is up to date
- `cargo clippy -- -D warnings` ensures that there are no compiler errors or warnings

### installation
You can set these commands to be run automatically before each commit by running `sh scripts/install-hooks.sh` on a unix-based OS. 
