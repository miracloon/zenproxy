#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
client_check.py

用途：
1. 测试本地某个代理端口是否可用
2. 测试访问目标网站时是否确实走了该代理
3. 对比直连出口 IP 与代理出口 IP，帮助判断代理是否生效

使用方式：
- 直接修改下方【配置区】
- 然后运行：
    uv run python tests/smoke/client_check.py
"""

from __future__ import annotations

import sys
import time
from typing import Optional, Dict, Any

import requests


# =============================================================================
# 配置区：直接改这里
# =============================================================================

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 60026

# 可选值：
# "http"
# "https"
# "socks5"
# "socks5h"
PROXY_SCHEME = "http"

# 你要测试访问的网站
TARGET_URL = "https://bing.com"

# 用来检测出口 IP 的服务
# 可选：
# "https://api.ipify.org?format=json"
# "https://httpbin.org/ip"
VERIFY_IP_URL = "https://api.ipify.org?format=json"

# 请求超时（秒）
TIMEOUT = 10

# 是否先做一次直连检测
CHECK_DIRECT = True


# =============================================================================
# 逻辑区：一般不用改
# =============================================================================

def build_proxy_url(scheme: str, host: str, port: int) -> str:
    return f"{scheme}://{host}:{port}"


def build_proxies(proxy_url: str) -> Dict[str, str]:
    return {
        "http": proxy_url,
        "https": proxy_url,
    }


def safe_json(resp: requests.Response) -> Optional[Dict[str, Any]]:
    try:
        return resp.json()
    except Exception:
        return None


def get_exit_ip(
    session: requests.Session,
    verify_url: str,
    proxies: Optional[Dict[str, str]],
    timeout: float,
) -> Dict[str, Any]:
    start = time.time()
    resp = session.get(verify_url, proxies=proxies, timeout=timeout)
    elapsed = time.time() - start

    data = safe_json(resp)
    text = resp.text.strip()

    ip = None
    if isinstance(data, dict):
        ip = data.get("ip") or data.get("origin") or data.get("query")
    if not ip:
        ip = text[:200]

    return {
        "ok": True,
        "status_code": resp.status_code,
        "elapsed_ms": round(elapsed * 1000, 2),
        "ip": ip,
        "body_preview": text[:300],
    }


def fetch_target(
    session: requests.Session,
    target_url: str,
    proxies: Optional[Dict[str, str]],
    timeout: float,
) -> Dict[str, Any]:
    start = time.time()
    resp = session.get(target_url, proxies=proxies, timeout=timeout, allow_redirects=True)
    elapsed = time.time() - start

    body_preview = resp.text[:300].replace("\n", " ").replace("\r", " ")

    return {
        "ok": True,
        "final_url": resp.url,
        "status_code": resp.status_code,
        "elapsed_ms": round(elapsed * 1000, 2),
        "server": resp.headers.get("Server"),
        "content_type": resp.headers.get("Content-Type"),
        "body_preview": body_preview,
    }


def try_request(func, *args, **kwargs) -> Dict[str, Any]:
    try:
        result = func(*args, **kwargs)
        result["error"] = None
        return result
    except requests.exceptions.ProxyError as e:
        return {"ok": False, "error": f"ProxyError: {e}"}
    except requests.exceptions.ConnectTimeout as e:
        return {"ok": False, "error": f"ConnectTimeout: {e}"}
    except requests.exceptions.ReadTimeout as e:
        return {"ok": False, "error": f"ReadTimeout: {e}"}
    except requests.exceptions.SSLError as e:
        return {"ok": False, "error": f"SSLError: {e}"}
    except requests.exceptions.ConnectionError as e:
        return {"ok": False, "error": f"ConnectionError: {e}"}
    except requests.exceptions.RequestException as e:
        return {"ok": False, "error": f"RequestException: {e}"}
    except Exception as e:
        return {"ok": False, "error": f"UnexpectedError: {e}"}


def print_line(char: str = "=", width: int = 80) -> None:
    print(char * width)


def print_title(title: str) -> None:
    print()
    print_line("=")
    print(title)
    print_line("=")


def print_kv(key: str, value: Any) -> None:
    print(f"{key:<20}: {value}")


def main() -> int:
    proxy_url = build_proxy_url(PROXY_SCHEME, PROXY_HOST, PROXY_PORT)
    proxies = build_proxies(proxy_url)

    session = requests.Session()
    session.headers.update(
        {
            "User-Agent": "proxy-test/1.0"
        }
    )

    # 很重要：避免系统环境变量里的 HTTP_PROXY / HTTPS_PROXY 干扰结果
    session.trust_env = False

    print_title("测试配置")
    print_kv("代理地址", proxy_url)
    print_kv("目标网站", TARGET_URL)
    print_kv("IP 检测地址", VERIFY_IP_URL)
    print_kv("请求超时", f"{TIMEOUT}s")

    direct_ip_result = None
    proxy_ip_result = None
    target_result = None

    if CHECK_DIRECT:
        print_title("1. 直连出口 IP")
        direct_ip_result = try_request(
            get_exit_ip,
            session,
            VERIFY_IP_URL,
            None,
            TIMEOUT,
        )
        if direct_ip_result["ok"]:
            print_kv("状态", "成功")
            print_kv("HTTP 状态码", direct_ip_result["status_code"])
            print_kv("出口 IP", direct_ip_result["ip"])
            print_kv("耗时", f'{direct_ip_result["elapsed_ms"]} ms')
        else:
            print_kv("状态", "失败")
            print_kv("错误", direct_ip_result["error"])

    print_title("2. 代理出口 IP")
    proxy_ip_result = try_request(
        get_exit_ip,
        session,
        VERIFY_IP_URL,
        proxies,
        TIMEOUT,
    )
    if proxy_ip_result["ok"]:
        print_kv("状态", "成功")
        print_kv("HTTP 状态码", proxy_ip_result["status_code"])
        print_kv("代理出口 IP", proxy_ip_result["ip"])
        print_kv("耗时", f'{proxy_ip_result["elapsed_ms"]} ms')
    else:
        print_kv("状态", "失败")
        print_kv("错误", proxy_ip_result["error"])

    print_title("3. 通过代理访问目标网站")
    target_result = try_request(
        fetch_target,
        session,
        TARGET_URL,
        proxies,
        TIMEOUT,
    )
    if target_result["ok"]:
        print_kv("状态", "成功")
        print_kv("最终 URL", target_result["final_url"])
        print_kv("HTTP 状态码", target_result["status_code"])
        print_kv("耗时", f'{target_result["elapsed_ms"]} ms')
        print_kv("Server", target_result["server"])
        print_kv("Content-Type", target_result["content_type"])
        print_kv("响应预览", target_result["body_preview"])
    else:
        print_kv("状态", "失败")
        print_kv("错误", target_result["error"])

    print_title("4. 判断结果")

    if not proxy_ip_result["ok"]:
        print("结论：代理请求失败。")
        print("说明这个端口大概率不可用，或者代理协议配置不对。")
        return 2

    if target_result["ok"]:
        print("结论：目标网站已经成功通过该代理访问。")
    else:
        print("结论：代理可能可用，但访问目标网站失败。")
        print("可能原因：")
        print("- 目标网站屏蔽该代理出口")
        print("- 该代理节点无法访问这个站")
        print("- 代理协议写错（例如本来是 socks5，却写成 http）")
        print("- 代理只支持部分请求类型")

    if direct_ip_result and direct_ip_result["ok"]:
        direct_ip = str(direct_ip_result["ip"])
        proxy_ip = str(proxy_ip_result["ip"])

        print()
        print(f"直连出口 IP：{direct_ip}")
        print(f"代理出口 IP：{proxy_ip}")

        if direct_ip != proxy_ip:
            print("判断：出口 IP 不同，说明请求大概率确实走了代理。")
        else:
            print("判断：出口 IP 相同。")
            print("这不一定代表没走代理，也可能是代理最终出口刚好和直连一致。")
    else:
        print("未拿到直连出口 IP，无法做直连对比。")
        print("但只要“代理出口 IP 成功”且“目标网站通过代理访问成功”，通常也能说明代理已生效。")

    return 0


if __name__ == "__main__":
    sys.exit(main())
