# imageWasher

ブラウザ内だけで画像を再エンコードし、メタデータを落とす Rust + WebAssembly ツールです。  
`EXIF`、GPS、`XMP`、`IPTC`、PNG テキストチャンク、AI 生成系の埋め込み情報を引き継がず、洗浄後の画像をそのままダウンロードできます。

## ブラウザ版

- 画像はサーバーへ送信しません
- 複数ファイルをまとめて処理できます
- ファイルごとにも、まとめて ZIP でもダウンロードできます
- PWA としてインストールできます
- 一度読み込むと、基本アセットはオフラインでも動きます
- GitHub Pages にそのまま載せられます

静的サイトのソースは [web/index.html](/Users/yanase/Documents/code/imageWasher/web/index.html)、Pages 向けの出力は `docs/` です。

## ローカルで Pages 用ファイルを作る

事前に必要:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.118 --locked
```

ビルド:

```bash
bash scripts/build-pages.sh
```

これで `docs/` に GitHub Pages へそのまま置ける静的ファイルが生成されます。

## GitHub Pages

GitHub Actions workflow は [.github/workflows/deploy-pages.yml](/Users/yanase/Documents/code/imageWasher/.github/workflows/deploy-pages.yml) に入っています。  
`main` ブランチへ push すると、GitHub Pages にデプロイされる構成です。

## CLI 版

ローカルでディレクトリを洗う CLI も残しています。

```bash
cargo run -- --input-dir ./input --output-dir ./output
```

## 対応形式

- JPEG
- PNG
- WEBP
- TIFF
- BMP
- GIF

## 制限

- アニメーション GIF は未対応です
- 再エンコードするので、JPEG などの不可逆形式は多少劣化することがあります
- 色プロファイルや埋め込みテキストも消える前提です
- メタデータを消しても、元画像より必ず軽くなるわけではありません
- 特に palette/indexed PNG は再エンコード時に通常の RGBA PNG へ展開されやすく、元より重くなることがあります
