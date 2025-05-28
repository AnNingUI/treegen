How to use：
```bash
# IN WIN CMD

git clone https://github.com/AnNingUI/treegen.git

cd treegen

cargo build --release && cargo install --path .

cd example

treegen .\tree.md .\tree.json .\tree.json5 .\tree.toml .\tree.yaml .\tree.yml
```

Note:
The markdown section can't be made content-filling because it's not friendly to AI reading and generation and deviates from my original development intent