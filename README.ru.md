# driftwm-wallpaper

Внешняя система обоев для [driftwm](https://github.com/roman/driftwm) через IPC.

Отдельное приложение для обоев, которое общается с driftwm через его IPC-сокет, предоставляя гибкие, GPU-ускоренные обои с живой синхронизацией камеры и зума.

## Возможности

- **IPC-интеграция**: Общается с driftwm через Unix-сокет для получения состояния камеры/зума в реальном времени
- **GPU-ускорение**: Использует wgpu для аппаратного ускорения рендеринга
- **Множество модулей**:
  - **Hyperbolic**: Мозаика {7,3} с анимированным панорамированием и настраиваемыми палитрами
  - **Solid**: Простой сплошной цвет фона
  - **Image**: Объи из изображения с режимами подгонки (cover, contain, fill, center, tile)
- **Горячая перезагрузка**: Изменения конфигурации применяются без перезапуска
- **Интеграция с темами**: Подписывается на тему Noctalia для автоматического обновления палитры
- **Layer Shell**: Рендерится как Wayland layer-shell поверхность на фоновом слое

## Установка

### Из исходного кода

```bash
cargo install --git https://github.com/roman/driftwm-wallpaper
```

Или сборка локально:

```bash
git clone https://github.com/roman/driftwm-wallpaper
cd driftwm-wallpaper
cargo build --release
./target/release/driftwm-wallpaper
```

## Конфигурация

Скопируйте `config.toml.example` в `~/.config/driftwm-wallpaper/config.toml` или `/etc/driftwm-wallpaper/config.toml` и настройте:

```toml
# Модуль обоев: "hyperbolic", "solid", "image"
module = "hyperbolic"

[modules.hyperbolic]
pan_rate = 0.15      # Радиан в секунду
rmax = 3.5           # Максимальный радиус
palette = { type = "named", value = "noctalia" }
interactive = true

[modules.solid]
color = "#1a1a2e"

[modules.image]
path = "~/Pictures/wallpaper.jpg"
fit = "cover"

[layer]
namespace = "driftwm-wallpaper"
z_order = -100
output = ""
anchors = ["top", "bottom", "left", "right"]
exclusive_zone = 0

[render]
fps = 60
vsync = true
power_preference = "high"

[theme]
enable = true
socket = ""
palette_key = "background"
```

## Использование

```bash
# Запуск с конфигурацией по умолчанию
driftwm-wallpaper

# Запуск с пользовательской конфигурацией
driftwm-wallpaper -c /путь/к/config.toml

# Включить отладочное логирование
driftwm-wallpaper --debug

# Вывести текущее состояние driftwm и выйти
driftwm-wallpaper --dump-state

# Переопределить модуль
driftwm-wallpaper -m solid
```

## Архитектура

```
┌─────────────────┐     IPC (Unix socket)      ┌──────────────────┐
│   driftwm       │ ◄─────────────────────────► │ driftwm-wallpaper │
│  (композитор)   │   События состояния, комманды│  (приложение обоев)│
└─────────────────┘                              └──────────────────┘
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │  wgpu Renderer   │
                                               │  (Vulkan/Metal/   │
                                               │   DX12/OpenGL)   │
                                               └──────────────────┘
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │ Wayland Layer    │
                                               │ Shell Surface    │
                                               │ (Background)     │
                                               └──────────────────┘
```

## Разработка модулей

Создавайте пользовательские модули обоев, реализуя трейт `WallpaperModule`:

```rust
pub trait WallpaperModule: Send + Sync {
    fn new(config: &HyperbolicConfig) -> Result<Self>;
    fn update(&mut self, frame: &FrameData);
    fn render(&mut self, renderer: &mut Renderer, frame: &FrameData) -> Result<TextureView>;
    fn resize(&mut self, width: u32, height: u32);
    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()>;
    fn name(&self) -> &str;
}
```

## Лицензия

GPL-3.0-or-later - подробности в [LICENSE](LICENSE).

## Участие в разработке

1. Форкните репозиторий
2. Создайте ветку для новой функции
3. Внесите изменения
4. Запустите тесты: `cargo test`
5. Отправьте pull request

## Связанные проекты

- [driftwm](https://github.com/roman/driftwm) - Композирующий оконный менеджер
- [Noctalia](https://github.com/roman/noctalia) - Система тем