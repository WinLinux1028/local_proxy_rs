# 概要
認証必須なプロキシを認証不要で使えるプロキシに変換します <br />
<img src="./assets/diagram.svg" /> <br />

# 用途
* 認証プロキシに対応しないアプリケーションをインターネットに接続する
* ログイン情報の漏洩防止(パスワードは平文で送られるので気休めだが､環境変数に書くよりはマシ)
* 単純なHTTPプロキシとしても機能し､簡単に実行出来るのでHTTPプロキシへの接続のデバッグに使う
* スマホのテザリング制限回避(スマホでTermuxなどでこのソフトを立ち上げ､テザリングされる側の端末でそのHTTPプロキシを使うよう設定)
* プロキシ環境下かつドメインベースでの検閲が行われている場合の検閲回避(DoHを設定する)

# 使い方
## 起動
`cargo run`とすれば実行できます <br />
`cargo run --release`とすると最適化されます <br />

## 設定
config.jsonをカレントディレクトリに置いてください <br />
例はconfig.json.exampleにあります <br />
<br />
上流のプロキシが指定されている場合､起動するとユーザー名とパスワードを聞かれた後､画面がクリアされます <br />

## 透過プロキシにする例
Linux環境で <br />
```json
{
    "doh_endpoint": "https://1.1.1.1/dns-query",
    "http_listen": ["127.0.0.1:8080", "[::1]:8080"],
    "dns_listen": ["127.0.0.1:8081", "[::1]:8081"],
    "tproxy_listen": {
        "listen": ["127.0.0.1:8081", "[::1]:8081"],
        "redir_type": "redirect"
    }
}
```
このように設定した場合､iptablesを以下のように設定します(uid-ownerは適宜変更) <br />
```bash
sudo iptables -A OUTPUT -m udp -p udp ! --dport 8081 -m owner --uid-owner 1000 -j REJECT
sudo ip6tables -A OUTPUT -m udp -p udp ! --dport 8081 -m owner --uid-owner 1000 -j REJECT
sudo iptables -t nat -A OUTPUT -m tcp -p tcp --dport 80 -m owner --uid-owner 1000 -j DNAT --to-destination 127.0.0.1:8080
sudo ip6tables -t nat -A OUTPUT -m tcp -p tcp --dport 80 -m owner --uid-owner 1000 -j DNAT --to-destination '[::1]:8080'
sudo iptables -t nat -A OUTPUT -m tcp -p tcp -m owner --uid-owner 1000 -j DNAT --to-destination 127.0.0.1:8081
sudo ip6tables -t nat -A OUTPUT -m tcp -p tcp -m owner --uid-owner 1000 -j DNAT --to-destination '[::1]:8081'
sudo iptables -t nat -A OUTPUT -m udp -p udp --dport 53 -m owner --uid-owner 1000 -j DNAT --to-destination 127.0.0.1:8081
sudo ip6tables -t nat -A OUTPUT -m udp -p udp --dport 53 -m owner --uid-owner 1000 -j DNAT --to-destination '[::1]:8081'
```