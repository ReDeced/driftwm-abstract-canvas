# driftwm-abstract-canvas

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
cargo install --git https://github.com/ReDeced/driftwm-abstract-canvas
```

Или сборка локально:

```bash
git clone https://github.com/ReDeced/driftwm-abstract-canvas
cd driftwm-abstract-canvas
cargo build --release
./target/release/driftwm-abstract-canvas
```

## Конфигурация

Скопируйте `config.toml.example` в `~/.config/driftwm-abstract-canvas/config.toml` или `/etc/driftwm-abstract-canvas/config.toml` и настройте:

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
namespace = "driftwm-abstract-canvas"
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
driftwm-abstract-canvas

# Запуск с пользовательской конфигурацией
driftwm-abstract-canvas -c /путь/к/config.toml

# Включить отладочное логирование
driftwm-abstract-canvas --debug

# Вывести текущее состояние driftwm и выйти
driftwm-abstract-canvas --dump-state

# Переопределить модуль
driftwm-abstract-canvas -m solid
```

## Интеграция с driftwm

Добавьте в конфигурацию driftwm (`~/.config/driftwm/config.toml` или аналогичный):

```toml
autostart = [
    "driftwm-abstract-canvas",
]

[background]
type = "none"
```

- `autostart` запускает обои при старте сессии driftwm.
- `[background] type = "none"` отключает встроенный фон driftwm, чтобы поверхность layer-shell обоев была видна.

## Архитектура

```
┌─────────────────┐     IPC (Unix socket)      ┌──────────────────┐
│   driftwm       │ ◄─────────────────────────► │ driftwm-abstract-canvas │
│  (композитор)   │   События состояния, команды│  (приложение обоев)│
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

## AI / ИИ

Этот проект был создан с использованием ИИ. Мы также рекомендуем использовать
инструменты ИИ при создании новых абстрактных модулей обоев для этого репозитория
— трейт `WallpaperModule` и WGSL-шейдеры хорошо подходят для кода, сгенерированного
ИИ, а файл `AGENTS.md` содержит рекомендации, помогающие ИИ-агентам писать
согласованные модули.

## Связанные проекты

- [driftwm](https://github.com/roman/driftwm) - Композирующий оконный менеджер
- [Noctalia](https://github.com/roman/noctalia) - Система тем