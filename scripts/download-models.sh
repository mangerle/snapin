#!/usr/bin/env bash
set -e

# Snapin PP-OCRv6 small 模型资产自动下载脚本
# 负责拉取 PP-OCRv6 small 检测与识别 ONNX 权重以及多语言字符字典配置

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "${SCRIPT_DIR}")"
TARGET_DIR="${PROJECT_ROOT}/assets/models/ppocrv6"
USER_DATA_DIR="${HOME}/.local/share/snapin/models/ppocrv6"

echo "=========================================================="
echo " Snapin PP-OCRv6 离线模型下载器"
echo " 目标项目路径: ${TARGET_DIR}"
echo "=========================================================="

mkdir -p "${TARGET_DIR}/det" "${TARGET_DIR}/rec"

BASE_MIRROR="https://hf-mirror.com"
BASE_OFFICIAL="https://huggingface.co"

download_file() {
    local repo="$1"
    local filename="$2"
    local dest="$3"

    if [ -f "${dest}" ] && [ -s "${dest}" ]; then
        echo "[已存在] ${dest}，跳过下载。"
        return 0
    fi

    echo "[正在下载] ${filename} 到 ${dest} ..."
    local url="${BASE_MIRROR}/${repo}/resolve/main/${filename}"
    if ! curl -L --fail --retry 3 --connect-timeout 10 -o "${dest}" "${url}"; then
        echo "[警告] 镜像下载失败，正在切换至官方源重试: ${url} -> ${BASE_OFFICIAL}"
        url="${BASE_OFFICIAL}/${repo}/resolve/main/${filename}"
        curl -L --fail --retry 3 --connect-timeout 15 -o "${dest}" "${url}"
    fi

    if [ ! -s "${dest}" ]; then
        echo "[错误] 文件 ${dest} 下载为空或失败！"
        exit 1
    fi
}

# 1. 下载 PP-OCRv6 small 检测模型及配置
echo ""
echo "--- 正在获取 PP-OCRv6 文本检测模型 (det) ---"
download_file "PaddlePaddle/PP-OCRv6_small_det_onnx" "inference.onnx" "${TARGET_DIR}/det/inference.onnx"
download_file "PaddlePaddle/PP-OCRv6_small_det_onnx" "inference.json" "${TARGET_DIR}/det/inference.json"
download_file "PaddlePaddle/PP-OCRv6_small_det_onnx" "inference.yml" "${TARGET_DIR}/det/inference.yml"

# 2. 下载 PP-OCRv6 small 文本识别模型及字典配置
echo ""
echo "--- 正在获取 PP-OCRv6 文本识别模型 (rec) ---"
download_file "PaddlePaddle/PP-OCRv6_small_rec_onnx" "inference.onnx" "${TARGET_DIR}/rec/inference.onnx"
download_file "PaddlePaddle/PP-OCRv6_small_rec_onnx" "inference.json" "${TARGET_DIR}/rec/inference.json"
download_file "PaddlePaddle/PP-OCRv6_small_rec_onnx" "inference.yml" "${TARGET_DIR}/rec/inference.yml"

# 3. 建立 ~/.local/share/snapin/models/ppocrv6 软链接便于本地直接运行调试
echo ""
echo "--- 正在配置本地开发与调试软链接 ---"
mkdir -p "$(dirname "${USER_DATA_DIR}")"
rm -rf "${USER_DATA_DIR}"
ln -sf "${TARGET_DIR}" "${USER_DATA_DIR}"

echo ""
echo "=========================================================="
echo " 模型下载与配置就绪！"
echo " 资产总览:"
ls -lh "${TARGET_DIR}/det"
ls -lh "${TARGET_DIR}/rec"
echo " 用户数据软链接: ${USER_DATA_DIR} -> $(readlink -f "${USER_DATA_DIR}")"
echo "=========================================================="
