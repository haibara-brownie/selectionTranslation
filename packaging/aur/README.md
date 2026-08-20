# AUR 打包

这里是发布到 [AUR](https://aur.archlinux.org) 的 `seltrans` 包。AUR 本身是一个独立的 git 仓库，这份是镜像，改完记得两边都推。

**0.3.0 起打的是 Tauri 2 版**（workspace 成员 `src-tauri`，二进制 `seltrans-tauri`，装成 `/usr/bin/seltrans`）。老的 GTK4/libadwaita 版停在 `v0.1.0-gtk` tag，不再维护。

迁移期曾经刻意让这份 PKGBUILD 停在 GTK 版不动（那时 Tauri 版功能还没追平，换过去等于让已装用户倒退）。三平台可用之后已经切完，那段权衡不再适用，删掉了。

### 切换时纠正的两处错判

迁移期在 PKGBUILD 顶部写下的切换清单，真做的时候发现有两条是错的：

- **不需要 `libayatana-appindicator`。** Linux 托盘走 ksni（纯 D-Bus StatusNotifierItem），压根不链接 appindicator 那套 C 库。`readelf -d` 查二进制的 NEEDED，里面连 `dbus` 之外的托盘相关库都没有。
- **不需要为 `pnpm install` 做离线依赖。** 当时以为「AUR 的构建环境不允许联网」，所以要靠 `source=()` 预下 tarball 喂 pnpm——这个前提就不成立：AUR 包是用户在自己机器上 `makepkg` 构建的，网络是通的，`makechrootpkg` 的 chroot 也不隔离网络。Arch 的 Rust 打包惯例本身就在 `prepare()` 里跑 `cargo fetch --locked`。照抄同一套即可：**联网都放在 `prepare()`，`build()` 全离线**（`cargo --frozen` + 已装好的 `node_modules`）。原以为是最大的一块工作量，实际是两行。

## 本地验证

```bash
cd packaging/aur
makepkg -f            # 构建（依赖已装的话不用 -s）
makepkg --printsrcinfo > .SRCINFO
```

产物 `*.pkg.tar.zst` 和 `src/` `pkg/` 目录都在 `.gitignore` 里。

`namcap seltrans-*.pkg.tar.zst` 可以再查一遍打包规范问题（需要 `pac namcap`）。

## 发版流程

整体的发版步骤（打 tag、CI 出三平台安装包、GitHub Release）见 [`docs/发版.md`](../../docs/发版.md)。下面只是其中 AUR 这一段。

1. 改版本号，**三处都要**：`Cargo.toml`（workspace.package）、`package.json`、`src-tauri/tauri.conf.json`
2. 打 tag 并推：`git tag -a vX.Y.Z -m "..." && git push origin vX.Y.Z`
3. 算新 tarball 的哈希：

   ```bash
   curl -sL "https://github.com/haibara-brownie/selectionTranslation/archive/refs/tags/vX.Y.Z.tar.gz" | sha256sum
   ```

4. 更新 `PKGBUILD` 的 `pkgver` 和 `sha256sums`，`pkgrel` 重置为 1
5. `makepkg -f` 验证，`makepkg --printsrcinfo > .SRCINFO`
6. 推到 AUR（见下）

## 首次推到 AUR（等有账号之后）

**现在还推不了**：AUR 的注册通道当时是关的，账号还没注册上。下面这套流程是等注册开放、账号到手之后照着走的，在那之前 `packaging/aur/` 里的东西只能本地 `makepkg` 自用。

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
git commit -m "初始提交：seltrans 0.3.0"
git push
```

**AUR 仓库里只能有 `PKGBUILD` 和 `.SRCINFO`**（外加可选的 `.install`、补丁文件），不要把源码推上去。

## 已知的打包坑

`options=('!lto')` 不能去掉。makepkg 默认往 `CFLAGS` 里加 `-flto=auto`，会把 `aws-lc-sys`（rustls 的加密后端）的 C 目标文件编成 LLVM bitcode，Rust 链接时报一堆 `undefined symbol: aws_lc_0_44_0_*` 而失败。
