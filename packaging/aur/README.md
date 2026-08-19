# AUR 打包

这里是发布到 [AUR](https://aur.archlinux.org) 的 `seltrans` 包。AUR 本身是一个独立的 git 仓库，这份是镜像，改完记得两边都推。

当前这份 PKGBUILD 打的是 **GTK4 版**（仓库根 package `seltrans`）。项目正在往 Tauri 2 迁移，迁移期的处置见下。

## 迁移期怎么办

**结论：现在只记录 Tauri 版需要什么，不动包结构。** 等 Tauri 版功能追平 GTK 版（迁移方案的 P5）再一次性切过去。切换清单写在 `PKGBUILD` 顶部的注释里。

三个方案都掂量过，选「先不动」是因为它对已经装了这个包的用户破坏最小：

| 方案 | 问题 |
|---|---|
| 现在就把 `depends` 换成 Tauri 那套 | 最糟。已装用户一跑 `-Syu` 就会被拉进 `webkit2gtk-4.1` + `gtk3` + `libayatana-appindicator` 一大串新依赖，换来的却是一个功能还不全的二进制——日常在用的工具直接倒退。 |
| 拆成 `seltrans` 和 `seltrans-tauri` 两个包 | 要多注册一个 AUR 包名，而现在**连 AUR 账号都还没有**（注册通道关着）。等于把一件做不了的事排在前面。 |
| 一个包装两个二进制 | `depends` 变成两套的并集（gtk4 + libadwaita + webkit2gtk + gtk3 + appindicator），依赖和体积都翻倍，用户为用不到的那半份付代价；而且 P5 删掉 GTK 代码时还得再改回去，白折腾一轮。 |

附带的好处：只加注释、不碰任何 `pkgname` / `depends` / `source` 字段，`.SRCINFO` 不用重新生成，也就不会和 AUR 上的版本对不上。

Tauri 版真正切过去时，除了换 `depends` / `makedepends`，最花时间的是**离线依赖**：AUR 的构建环境不允许联网，`pnpm install` 得靠 `source=()` 预先下好的 tarball 或离线镜像来喂。这条别等到切换当天才发现。

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

1. 改 `Cargo.toml` 里的 `version`
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
git commit -m "初始提交：seltrans 0.1.0"
git push
```

**AUR 仓库里只能有 `PKGBUILD` 和 `.SRCINFO`**（外加可选的 `.install`、补丁文件），不要把源码推上去。

## 已知的打包坑

`options=('!lto')` 不能去掉。makepkg 默认往 `CFLAGS` 里加 `-flto=auto`，会把 `aws-lc-sys`（rustls 的加密后端）的 C 目标文件编成 LLVM bitcode，Rust 链接时报一堆 `undefined symbol: aws_lc_0_44_0_*` 而失败。
