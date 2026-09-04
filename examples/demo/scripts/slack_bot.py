#!/usr/bin/env python3
import sys
import json
import requests
import time
import os

# 配置 Slack Webhook URL
WEBHOOK_URL = "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX"

def send_alert(event_data):
    """发送告警到 Slack"""
    try:
        payload = {
            "text": f"🚨 *Process Alert*: {event_data.get('program_name', 'Unknown')}",
            "attachments": [
                {
                    "color": "#ff0000" if "FATAL" in event_data.get('event', '') else "#warning",
                    "fields": [
                        {"title": "Event", "value": event_data.get('event'), "short": True},
                        {"title": "Hostname", "value": event_data.get('hostname'), "short": True},
                        {"title": "Message", "value": event_data.get('msg'), "short": False}
                    ]
                }
            ]
        }

        # 设置超时，防止网络卡顿导致脚本阻塞，进而阻塞 stdin 管道
        resp = requests.post(WEBHOOK_URL, json=payload, timeout=2.0)
        if resp.status_code != 200:
            sys.stderr.write(f"Slack API Error: {resp.status_code} {resp.text}\n")

    except Exception as e:
        # 写入 stderr，这些日志通常会被 Super 捕获并记录到文件
        sys.stderr.write(f"Failed to send alert: {str(e)}\n")

def main():
    # 强制让 stdout/stderr 不缓存，实时输出（可选，但在调试时很有用）
    # sys.stdout.reconfigure(line_buffering=True)

    sys.stderr.write("Slack Bot Listener Started...\n")

    # === 核心循环 ===
    # sys.stdin 是一个迭代器，它会阻塞等待，直到有新的一行数据进来
    # Super 发送数据时，必须以 \n (换行符) 结尾，形成 JSON Lines 格式
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            # 1. 解析 JSON
            event = json.loads(line)

            # 2. 过滤感兴趣的事件
            # 假设 Super 发来的 JSON 包含 'event' 字段
            event_type = event.get("event", "")

            if event_type in ["process_fatal", "process_backoff"]:
                send_alert(event)

            # (可选) 如果协议需要 ACK，可以 print 到 stdout
            # print("RESULT 2\nOK")
            # sys.stdout.flush() # 必须 flush，否则 Super 此时可能收不到

        except json.JSONDecodeError:
            sys.stderr.write(f"Invalid JSON received: {line}\n")
        except Exception as e:
            sys.stderr.write(f"Unexpected Error: {str(e)}\n")

if __name__ == "__main__":
    main()
