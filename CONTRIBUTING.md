# Contributing

Read the Vistoda family [contribution guide](https://github.com/luigibarretta/vistoda-home-assistant/blob/main/CONTRIBUTING.md)
before changing a cross-repository contract.

This repository owns the Blink Rust provider app and Blink Home Assistant
adapter. Shared panel behavior belongs in `vistoda-home-assistant`; app-store
metadata belongs in `vistoda-addons`.

Run the validation commands in [README.md](README.md#development). Tests must not
wake battery cameras, trigger provider throttling or mutate retained USB media.
Report security issues through [SECURITY.md](SECURITY.md), not a public issue.
