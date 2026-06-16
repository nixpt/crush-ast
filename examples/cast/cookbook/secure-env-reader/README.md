# secure-env-reader

Demonstrates `ImportStatement::SecureEnv` — importing encrypted environment
secrets into a capsule, both forms:

- **Selective**: `"keys": ["API_TOKEN", "DB_URL"]` — each key is read via the
  `secrets.read` capability and stored under its own name.
- **Bulk with alias**: `"keys": []` imports all secrets as one module object
  under `alias`; `db_path` overrides the default secrets database.

The imported `API_TOKEN` is then consumed inside a Python `LangBlock` via the
`variables` injection list (semantic-analyzer scopes don't see runtime import
stores, so LangBlock injection is the proven consumption path today).

```bash
ant crush cast validate main.cast.json
ant crush compile --from-cast main.cast.json -o secure-env-reader.casmb
```
