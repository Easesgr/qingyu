# qingyu-monitor:v4 — installer helper, run only after an explicit user action.
"""Use Hermes' YAML parser; reject malformed or concurrent edits before any write."""
from io import StringIO
import copy
import os
from pathlib import Path
import stat
import sys
import tempfile
import time


def _yaml_editor():
    try:
        import hermes_yaml
    except ImportError:
        import yaml  # Older Hermes releases used PyYAML.

        class UniqueLoader(yaml.SafeLoader):
            pass

        def mapping(loader, node, deep=False):
            loader.flatten_mapping(node)
            result = {}
            for key_node, value_node in node.value:
                key = loader.construct_object(key_node, deep=deep)
                if not isinstance(key, str) or key in result:
                    raise ValueError("配置包含重复或非文本设置名")
                result[key] = loader.construct_object(value_node, deep=deep)
            return result

        UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, mapping)
        return lambda raw: yaml.load(raw, Loader=UniqueLoader), lambda value: yaml.safe_dump(value, allow_unicode=True, sort_keys=False, width=2**31-1)

    editor = hermes_yaml.roundtrip_yaml()

    def load(raw):
        checked = hermes_yaml.safe_load(raw)  # Reject duplicate keys and custom tags.
        value = editor.load(raw)
        if value != checked:
            raise ValueError("无法无损解析 Hermes 配置")
        return value

    def dump(value):
        target = StringIO()
        editor.dump(value, target)
        return target.getvalue()

    return load, dump


def _text_keys(value):
    if isinstance(value, dict):
        if any(not isinstance(key, str) for key in value):
            raise ValueError("配置包含非文本设置名")
        for item in value.values():
            _text_keys(item)
    elif isinstance(value, list):
        for item in value:
            _text_keys(item)


def _backup(path, raw):
    if not raw:
        return
    directory = path.parent / ".qingyu-backups"
    directory.mkdir(mode=0o700, exist_ok=True)
    info = directory.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError("配置备份目录不是当前用户的私有目录")
    fd, _ = tempfile.mkstemp(prefix=path.name + "." + str(time.time_ns()) + ".", suffix=".bak", dir=directory)
    with os.fdopen(fd, "wb") as target:
        os.fchmod(target.fileno(), 0o600)
        target.write(raw)
        target.flush()
        os.fsync(target.fileno())


def main():
    load, dump = _yaml_editor()
    path, operation = Path(sys.argv[1]), sys.argv[2]
    if operation not in {"check", "install", "uninstall"}:
        raise ValueError("不支持的配置操作")
    raw = b""
    if path.exists() or path.is_symlink():
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_size > 2 * 1024 * 1024:
            raise ValueError("配置不是普通文件或超过大小限制")
        raw = path.read_bytes()
    config = load(raw) if raw else {}
    _text_keys(config)
    if not isinstance(config, dict):
        raise ValueError("配置顶层必须是设置对象")
    plugins = config.get("plugins", {})
    if not isinstance(plugins, dict):
        raise ValueError("plugins 必须是设置对象")
    for key in ("enabled", "disabled"):
        values = plugins.get(key, [])
        if not isinstance(values, list) or any(not isinstance(v, str) for v in values):
            raise ValueError("plugins." + key + " 必须是名称列表")
    name = "qingyu-monitor"
    enabled = plugins.get("enabled", [])
    installed = name in enabled and name not in plugins.get("disabled", [])
    if operation == "check":
        print("installed" if installed else "absent")
        return
    if operation == "install" and name in plugins.get("disabled", []):
        raise ValueError("qingyu-monitor 已在 Hermes 中明确禁用，请先移除该禁用项")
    replacement = [v for v in enabled if v != name]
    if operation == "install":
        replacement.append(name)
    if replacement == enabled or (operation == "uninstall" and name not in enabled):
        return
    # Detach YAML aliases before editing: an enabled list or plugins object can
    # also be referenced by unrelated settings, which must retain their values.
    plugins = copy.copy(plugins)
    if hasattr(plugins, "yaml_set_anchor"):
        plugins.yaml_set_anchor(None)
    if "enabled" in plugins:
        values = copy.copy(plugins["enabled"])
        values[:] = replacement
        if hasattr(values, "yaml_set_anchor"):
            values.yaml_set_anchor(None)
        plugins["enabled"] = values
    else:
        plugins["enabled"] = replacement
    config["plugins"] = plugins
    body = dump(config).encode("utf-8")
    # Reparse the complete output before replacement; no secrets/config text go to stdout.
    if load(body) != config:
        raise ValueError("无法无损保存 Hermes 配置")
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".qingyu-", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "wb") as target:
            target.write(body)
            target.flush()
            os.fsync(target.fileno())
        if (path.read_bytes() if path.exists() else b"") != raw:
            raise ValueError("配置已被其他程序修改，请重试")
        _backup(path, raw)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Parser diagnostics can contain credential lines; report only the error category.
        message = str(error) if isinstance(error, ValueError) else "配置解析失败或无法写入（" + type(error).__name__ + "）"
        print(message, file=sys.stderr)
        sys.exit(1)
