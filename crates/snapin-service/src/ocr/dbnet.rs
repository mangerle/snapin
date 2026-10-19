//! DBNet (Differentiable Binarization) 文本检测二值化与多边形后处理。

/// 检测提取到的文本行元组：外接矩形与四边形角点
pub type DetectedBox = ((f32, f32, f32, f32), [(f32, f32); 4]);

/// 从 DBNet 输出的概率特征图中提取文本行边界框与多边形
pub fn extract_bounding_boxes(
    prob_map: &[f32],
    map_w: usize,
    map_h: usize,
    threshold: f32,
    orig_w: u32,
    orig_h: u32,
) -> Vec<DetectedBox> {
    if map_w == 0 || map_h == 0 || prob_map.len() < map_w * map_h {
        return Vec::new();
    }

    let mut visited = vec![false; map_w * map_h];
    let mut boxes = Vec::new();

    let scale_x = orig_w as f32 / map_w as f32;
    let scale_y = orig_h as f32 / map_h as f32;

    for y in 0..map_h {
        for x in 0..map_w {
            let idx = y * map_w + x;
            if visited[idx] || prob_map[idx] < threshold {
                continue;
            }

            // BFS 搜索连通分量 (Connected Components)
            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut area = 0;

            let mut queue = std::collections::VecDeque::new();
            queue.push_back((x, y));
            visited[idx] = true;

            while let Some((cx, cy)) = queue.pop_front() {
                area += 1;
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);

                // 4 邻域扫描
                let neighbors = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];

                for (nx, ny) in neighbors {
                    if nx < map_w && ny < map_h {
                        let n_idx = ny * map_w + nx;
                        if !visited[n_idx] && prob_map[n_idx] >= threshold {
                            visited[n_idx] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }

            // 过滤噪点 (面积小于 20 像素或宽高极小)
            let box_w = max_x - min_x + 1;
            let box_h = max_y - min_y + 1;
            if area < 20 || box_w < 4 || box_h < 4 {
                continue;
            }

            // 坐标按原图比例反变换，并适当外扩 2 个像素 (DBNet unclip 补偿)
            let real_x = (min_x as f32 * scale_x - 2.0).max(0.0);
            let real_y = (min_y as f32 * scale_y - 2.0).max(0.0);
            let real_w = ((box_w as f32 * scale_x) + 4.0).min(orig_w as f32 - real_x);
            let real_h = ((box_h as f32 * scale_y) + 4.0).min(orig_h as f32 - real_y);

            let bbox = (real_x, real_y, real_w, real_h);
            let polygon = [
                (real_x, real_y),
                (real_x + real_w, real_y),
                (real_x + real_w, real_y + real_h),
                (real_x, real_y + real_h),
            ];

            boxes.push((bbox, polygon));
        }
    }

    // 按照从上到下、从左到右排序 (符合人类阅读顺序)
    boxes.sort_by(|a, b| {
        let y_diff = a.0 .1 - b.0 .1;
        if y_diff.abs() > 10.0 {
            y_diff
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a.0 .0
                .partial_cmp(&b.0 .0)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    boxes
}
