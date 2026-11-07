# Plan de pruebas

## Pruebas automaticas

Ejecutar antes de cada entrega:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Las pruebas unitarias cubren la extraccion de frecuencia de tramas `FA` e `IF`,
el rechazo de tramas invalidas y la construccion del comando enviado al Expert.

## Pruebas con hardware

1. Iniciar con ambos equipos apagados y comprobar los reintentos sin consumo
   creciente de memoria o CPU.
2. Encender primero el K3 y despues el Expert; ambos deben reconectar sin
   reiniciar CAT Pacer.
3. Generar actividad CAT externa y comprobar que no aparece polling adicional.
4. Cambiar rapidamente de frecuencia y verificar que el Expert recibe como
   maximo un comando por cada intervalo `tx_time`.
5. Detener la actividad CAT y verificar `FA;` tras vencer `ex_data`.
6. Desconectar y reconectar cada cable serie por separado.
7. Enviar tramas parciales, concatenadas, desconocidas y con bytes corruptos.
8. Ejecutar durante al menos 24 horas observando CPU, memoria y continuidad.

## Criterios minimos

- Ningun bloqueo permanente tras perder una respuesta o un puerto.
- Sin crecimiento sostenido de memoria.
- Sin polling mientras llegan frecuencias validas dentro de `ex_data`.
- Cadencia hacia el Expert nunca superior a `tx_time`.
- Recuperacion automatica al restaurar cada puerto.

El uso de la expresion "mision critica" requiere completar estas pruebas con el
hardware real y documentar los resultados.
