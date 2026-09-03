---
title: "Environment & Secrets"
weight: 3
description: "Securely manage and mount sensitive credentials without crypto-shredding risks."
---

Managing environment variables is easy, but managing **secrets** (like database passwords and API keys) is notoriously difficult. 

If a process manager saves secrets as plain text in its internal state file, anyone with read access to the server can steal them. If it encrypts them using a master key, losing that key means all your configurations are permanently destroyed (crypto-shredding).

Super solves this elegantly using a **Reference & Masking** architecture.

## 1. Dynamic Environment Mounting

Instead of passing secrets directly via the CLI, you should store them in a standard `.env` file and configure Super to *reference* that file.

```bash
# Good: Super only saves the file path
super add --name api-server --env-file /etc/secrets/prod.env ./api-server

# Bad: Secrets are passed directly and will be saved in the config state
super add --name api-server -e DB_PASSWORD=super_secret ./api-server
```

**How it works:**
When you use `--env-file`, Super does **not** read the file during configuration. Instead, it saves the path (`/etc/secrets/prod.env`) into its internal state. 
When the process is about to be spawned, Super dynamically reads the file, parses the variables, and injects them directly into the child process's memory.

**Benefits:**
* **Zero Persistence**: Your secrets never touch Super's `snapshot.json`.
* **Instant Updates**: If you rotate passwords by editing `/etc/secrets/prod.env`, you only need to run `super restart api-server`. No need to update the process configuration.
* **Unix Security**: You can protect your secrets using standard OS file permissions (e.g., `chmod 600 /etc/secrets/prod.env`).

## 2. Display Masking

If you must pass secrets directly via the CLI or API (`-e KEY=VAL`), Super protects you from "shoulder surfing" and accidental screenshot leaks.

When using `super info` or checking the Dashboard, Super automatically masks the values of environment variables whose keys contain any of the following keywords:
`SECRET`, `PASSWORD`, `TOKEN`, `KEY`, or `CREDENTIAL`.

```bash
$ super info api-server

Environment:
  PORT=8080
  DB_HOST=10.0.0.5
  DB_PASSWORD=********    # <--- Automatically masked
  STRIPE_API_KEY=******** # <--- Automatically masked
```

## 3. Strict State File Permissions

As an ultimate fallback layer, the Super daemon ensures that its persistent state file (`data/snapshot.json`) is strictly secured.

On Unix systems, Super forces the file permissions to `0600` upon every write. This guarantees that only the user running the `superd` daemon (usually `root`) can read the underlying process configurations.