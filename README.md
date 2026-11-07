# CAT Pacer

CAT Pacer es un dosificador de frecuencia CAT muy ligero para colocar entre un
Elecraft K3 y un amplificador Expert 1K3FA.

Escucha las respuestas `IF` y `FA` que produce la actividad CAT existente,
conserva la frecuencia mas reciente y la envia al amplificador a un ritmo
controlado. Solo consulta al K3 mediante `FA;` cuando la informacion disponible
ha caducado.

## Origen y evolucion

CAT Pacer es la evolucion en Rust de CAT Bridge, el dosificador CAT original
escrito en Python por EB1TR. Conserva su objetivo: aprovechar la actividad CAT
del Elecraft K3 y regular el envio de frecuencia al Expert 1K3FA, generando
polling solo cuando sea necesario.

El codigo Python original, sus variantes y archivos de PyInstaller permanecen
en el [commit inicial `0806926`](https://github.com/EB1TR/cat-pacer/tree/08069261e9b8efacdc2f4760126b6a6697d7c197)
como referencia historica. La version activa se desarrolla en Rust.

## Caracteristicas

- Un unico bucle CAT y un hilo ligero para la interfaz nativa de Windows.
- Sin interprete, servicios externos ni tareas en segundo plano.
- Temporizacion monotona, memoria acotada y sin expresiones regulares.
- Reintento de polling y reconexion de puertos serie.
- Configuracion validada antes de comenzar.
- Dependencias: `serde`, `toml` (solo lectura), `serialport` y, en Windows,
  `windows-sys` para la consola, la ventana y la bandeja.

## Ventana y bandeja de Windows

Al ejecutar el programa se abre un monitor de solo lectura con tema oscuro
[Dracula](https://draculatheme.com/contribute), barra de titulo y controles de
cerrar nativo de Windows, sin boton de minimizar. Usa Segoe UI para los textos y Consolas
para la frecuencia. La informacion se agrupa en frecuencia, conexiones del K3
y el Expert, dosificacion y actividad:

- Frecuencia recibida del K3, en MHz y con precision de un hercio.
- Estado de vigencia de la informacion y conexiones de ambos puertos.
- Ultima frecuencia enviada al Expert.
- Cadencia, vigencia y activacion del polling configuradas.
- Contadores de respuestas CAT, envios y consultas, y ultimo evento.

La ventana se puede redimensionar y colocar en zonas de FancyZones. La
distribucion responde al area disponible: conexiones en dos columnas cuando
hay anchura y apiladas en ventanas estrechas, tipografia compacta cuando falta
espacio y desplazamiento vertical nativo si no cabe todo el contenido. Se
puede desplazar con la rueda o con la barra lateral. El marco mantiene solo
la X; no se recupera el boton de minimizar.

Cerrar con la X guarda la ventana en la bandeja y solicita una notificacion
nativa de que CAT Pacer sigue ejecutandose en segundo plano, sin detener CAT.
Un clic en el icono la restaura; el boton derecho ofrece
**Mostrar CAT Pacer** y **Salir**. **Salir** detiene el proceso y libera los
puertos. El tooltip del icono muestra la frecuencia y el estado del K3.

El ejecutable, la ventana y la bandeja comparten el mismo icono incrustado.
`build.rs` genera sus versiones de 16, 32 y 48 px y las incorpora al EXE.
`winresource` se usa unicamente al compilar; no añade dependencias de ejecucion.

La interfaz usa controles Win32, sin frameworks graficos. Tiene su propio hilo
para que mover la ventana o mantener abierto el menu no pause los envios CAT.
El bucle serie publica el estado como maximo cinco veces por segundo por una
cola de un solo elemento, sin esperar al monitor. Solo se repintan los textos
que cambian. El icono se vuelve a registrar si Explorer se reinicia.
La redistribucion solo se calcula al cambiar el tamaño o desplazar la ventana;
las fuentes se crean una sola vez y se reutilizan, sin trabajo adicional en el
bucle CAT por FancyZones.

Al abrir cualquier ejecutable Windows con doble clic no se crea una consola.
En desarrollo, `cargo run` aprovecha la terminal desde la que se ejecuta para
mostrar diagnosticos. La version release omite los mensajes rutinarios de
terminal. Los errores que impiden iniciar el programa se muestran en un
dialogo, incluso cuando no hay consola.

Solo se permite una instancia por sesion de Windows, incluso si se combinan
debug, release o copias en carpetas diferentes. Al abrirlo de nuevo se muestra
la ventana existente y el segundo proceso termina antes de cargar configuracion
o abrir puertos. Se usa un mutex nativo, que Windows libera al finalizar el
proceso, sin archivos de bloqueo ni comprobaciones periodicas en el bucle CAT.

## Compilar

Instala el toolchain estable de Rust y ejecuta:

```powershell
cargo build --release
```

El ejecutable se genera en `target/release/cat-pacer.exe`.

## Configurar

Copia `config.example.toml` como `config.toml` junto a `cat-pacer.exe` y adapta
los puertos y velocidades. Tambien se puede indicar una ruta explicita:

```powershell
cat-pacer.exe C:\ruta\config.toml
```

TOML usa pares `nombre = valor` y permite comentarios que empiezan con `#`.
Los puertos se escriben entre comillas, los tiempos en segundos admiten
decimales y las opciones booleanas usan `true` o `false`, sin comillas:

```toml
# Intervalo minimo entre envios: 300 milisegundos.
tx_time = 0.3
polling = true
```

Se requieren todos los campos. Los nombres desconocidos y los tipos o valores
incorrectos se rechazan antes de abrir puertos. La configuracion se lee una vez
al iniciar; tras editarla hay que reiniciar el programa. El archivo local
`config.toml` esta ignorado por Git.

Para desarrollo, indicando el archivo local de forma explicita:

```powershell
cargo run -- config.toml
```

Al ejecutar `cargo run` en desarrollo, pulsa **Ctrl+C** en la terminal para
detener el programa. Desde la interfaz, utiliza **Salir** en el menu de
bandeja. El bucle termina, Rust libera ambos puertos y la aplicacion devuelve
el codigo de salida `0`.

Las configuraciones JSON de versiones anteriores deben convertirse a TOML;
la aplicacion ya no lee `config.json`.

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

Consulta [PROTOCOL.md](PROTOCOL.md) para el comportamiento exacto y
[TESTING.md](TESTING.md) como guia para las comprobaciones manuales del usuario.
