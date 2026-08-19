# AUR 打包

这里是发布到 [AUR](https://aur.archlinux.org) 的 `seltrans` 包。AUR 本身是一个独立的 git 仓库，这份是镜像，改完记得两边都推。

## 本地验证

```bash
cd packaging/aur
makepkg -f            # 构建（依赖已装的话不用 -s）
makepkg --printsrcinfo > .SRCINFO
```

产物 `*.pkg.tar.zst` 和 `src/` `pkg/` 目录都在 `.gitignore` 里。

`namcap seltrans-*.pkg.tar.zst` 可以再查一遍打包规范问题（需要 `pac namcap`）。

## 发版流程

1. 改 `Cargo.toml` 里的 `version`
2. 打 tag 并推：`git tag -a vX.Y.Z -m "..." && git push origin vX.Y.Z`
3. 算新 tarball 的哈希：

   ```bash
   curl -sL "https://github.com/haibara-brownie/selectionTranslation/archive/refs/tags/vX.Y.Z.tar.gz" | sha256sum
   ```

4. 更新 `PKGBUILD` 的 `pkgver` 和 `sha256sums`，`pkgrel` 重置为 1
5. `makepkg -f` 验证，`makepkg --printsrcinfo > .SRCINFO`
6. 推到 AUR（见下）

## 首次推到 AUR

需要一个 AUR 账号，并把 SSH 公钥填进账号设置。

```bash
# 1. 把公钥内容贴到 https://aur.archlinux.org/account/ 的 "SSH Public Key"
cat ~/.ssh/id_ed25519.pub

# 2. 配 SSH
cat >> ~/.ssh/config <<'EOF'
Host aur.archlinux.org
    User aur
    IdentityFile ~/.ssh/id_ed25519
EOF

# 3. 克隆空仓库（包名没被占用时会得到一个空仓库）
git clone ssh://aur@aur.archlinux.org/seltrans.git /tmp/aur-seltrans

# 4. 放进去推上去
cp packaging/aur/PKGBUILD packaging/aur/.SRCINFO /tmp/aur-seltrans/
cd /tmp/aur-seltrans
git add PKGBUILD .SRCINFO
git commit -m "初始提交：seltrans 0.1.0"
git push
```

**AUR 仓库里只能有 `PKGBUILD` 和 `.SRCINFO`**（外加可选的 `.install`、补丁文件），不要把源码推上去。

## 已知的打包坑

`options=('!lto')` 不能去掉。makepkg 默认往 `CFLAGS` 里加 `-flto=auto`，会把 `aws-lc-sys`（rustls 的加密后端）的 C 目标文件编成 LLVM bitcode，Rust 链接时报一堆 `undefined symbol: aws_lc_0_44_0_*` 而失败。
