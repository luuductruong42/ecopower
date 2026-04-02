# Ecopower

**Modern System Resource Monitor with History Charts**

Ecopower là công cụ giám sát tài nguyên hệ thống đẹp mắt, nhẹ và mạnh mẽ dành cho Linux (Ubuntu/Debian).  
Được viết hoàn toàn bằng **Rust** sử dụng thư viện **Ratatui**, Ecopower cung cấp giao diện terminal hiện đại với biểu đồ lịch sử thời gian thực.

![Ecopower Screenshot](assets/ecopower.png)

## Tính năng chính

- **Overview**: Gauge CPU & Memory + thông tin hệ thống (Hostname, Uptime, Load Average, Kernel…)
- **Processes**: Danh sách tiến trình chi tiết, hỗ trợ lọc, sắp xếp, kill và đổi nice
- **Disks**: Hiển thị thông tin ổ đĩa (tên, điểm mount, dung lượng, % sử dụng)
- **History**: Biểu đồ lịch sử CPU, Memory, Download và Upload trong ~75 giây gần nhất
- Giao diện tabs dễ sử dụng
- Hỗ trợ đầy đủ phím tắt và bảng Help

## Phím tắt

| Phím                  | Chức năng                                      |
|-----------------------|------------------------------------------------|
| `1` `2` `3` `4`       | Chuyển trực tiếp đến tab tương ứng             |
| `Tab`                 | Chuyển tab tiếp theo                           |
| `f`                   | Bật chế độ lọc theo tên process                |
| `c`                   | Sắp xếp Processes theo CPU                     |
| `m`                   | Sắp xếp Processes theo Memory                  |
| `k`                   | Kill process đang chọn                         |
| `n`                   | Đổi Nice value của process đang chọn           |
| `↑` `↓` / `j` `k`     | Di chuyển lên/xuống trong bảng Processes       |
| `h`                   | Bật/tắt bảng Help                              |
| `q` hoặc `Esc`        | Thoát chương trình                             |

## Cài đặt

### Cách 1: Cài từ gói .deb (Khuyến nghị)

Tải file `.deb` mới nhất từ [Releases](https://github.com/luuductruong42/ecopower/releases), sau đó chạy:

```bash
sudo dpkg -i ecopower_*.deb
sudo apt install -f

Chạy chương trình:
```bash
ecopower
Hoặc tìm Ecopower trong menu Applications.

### Cách 2: Build từ source
git clone https://github.com/luuductruong42/ecopower.git
cd ecopower
cargo build --release
./target/release/ecopower

## Sau khi cài đặt
### Xem License
cat /usr/share/doc/ecopower/copyright

### Xem README đầy đủ
less /usr/share/doc/ecopower/README.md

##Công nghệ sử dụng

Rust (Edition 2021)
Ratatui + Crossterm (TUI Framework)
Sysinfo (thu thập thông tin hệ thống)
Anyhow (xử lý lỗi)

##License
MIT License — xem chi tiết trong file LICENSE.

# Made by Đức Trường


