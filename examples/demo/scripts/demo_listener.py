#!/usr/bin/env python3
import sys
import json
import time

# 强制输出不缓存
sys.stderr.write("--> Python Listener Started! Waiting for events...\n")

while True:
    try:
        # 读取一行 (阻塞直到有数据)
        line = sys.stdin.readline()
        if not line:
            break # EOF (Super 关闭了)

        # 解析
        event = json.loads(line)

        # 打印到 stderr (会被 Super 记录到日志或显示在终端)
        event_type = event.get("type", "unknown")
        payload = event.get("payload", {})
        prog = payload.get("program_name", "N/A")

        sys.stderr.write(f"--> [PYTHON] Received Event: {event_type} from {prog}\n")
        sys.stderr.flush()

    except Exception as e:
        sys.stderr.write(f"--> [PYTHON] Error: {e}\n")
