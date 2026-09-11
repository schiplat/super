# Super Dashboard (OSS)

Vue 3 + Vite control plane embedded into `superd` (and optionally into the
licensed `ui` plugin as an override).

## Develop

```bash
cd dashboard
npm install
npm run dev          # http://localhost:5173 → proxies /api to :9002
```

## Build

```bash
cd dashboard
npm install
npm run build        # → dist/ (required before `cargo build -p superd`)
```

From the OSS repo root: `make frontend` then `make build`.

From `super-pro` (plugins still embed the same dist): `make frontend`.
