# CAT Pacer

CAT Pacer es un dosificador de frecuencia CAT muy ligero para colocar entre un
Elecraft K3 y un amplificador Expert 1K3FA.

Escucha las respuestas `IF` y `FA` que produce la actividad CAT existente,
conserva la frecuencia mas reciente y la envia al amplificador a un ritmo
controlado. Solo consulta al K3 mediante `FA;` cuando la informacion disponible
ha caducado.

## Caracteristicas

- Un unico proceso, hilo y bucle de control.
- Sin interprete, servicios externos ni tareas en segundo plano.
- Temporizacion monotona, memoria acotada y sin expresiones regulares.
- Reintento de polling y reconexion de puertos serie.
- Configuracion validada antes de comenzar.
- Dependencias minimas: `serde`, `serde_json` y `serialport`.

## Compilar

Instala el toolchain estable de Rust y ejecuta:

```powershell
cargo build --release
```

El ejecutable se genera en `target/release/cat-pacer.exe`.

## Configurar

Copia `config.example.json` como `config.json` junto a `cat-pacer.exe` y adapta
los puertos y velocidades. Tambien se puede indicar una ruta explicita:

```powershell
cat-pacer.exe C:\ruta\config.json
```

| Campo | Funcion |
|---|---|
| `rx_port` | Puerto conectado al CAT del K3 |
| `rx_baud` | Velocidad CAT del K3 |
| `tx_port` | Puerto de salida hacia el Expert |
| `tx_baud` | Velocidad CAT del Expert |
| `ex_data` | Segundos durante los que una frecuencia recibida sigue vigente |
| `tx_time` | Intervalo minimo, en segundos, entre envios al amplificador |
| `polling` | Permite solicitar `FA;` cuando no hay informacion vigente |
| `repeater` | Habilita el puerto y los envios al amplificador |

## Verificar

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Consulta [PROTOCOL.md](PROTOCOL.md) para el comportamiento exacto y
[TESTING.md](TESTING.md) antes de utilizarlo de forma continua.
