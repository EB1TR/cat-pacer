# Protocolo y comportamiento

## Topologia

```text
Actividad CAT existente -> Elecraft K3 -> CAT Pacer -> Expert 1K3FA
```

CAT Pacer no es un proxy CAT general. Observa las respuestas del K3 y solo
dosifica la frecuencia hacia el amplificador.

## Entrada

Se aceptan tramas ASCII terminadas en `;` con uno de estos prefijos:

- `FA` seguido de 11 digitos.
- `IF` seguido de 11 digitos; los campos posteriores se ignoran.

Las tramas desconocidas o mal formadas se descartan. El buffer de entrada esta
limitado a 4096 bytes para impedir crecimiento de memoria ante datos corruptos.

## Salida

La frecuencia se envia como `FA` seguido de 11 digitos y `;`. Nunca se envia
con mayor frecuencia que el intervalo `tx_time`.

Una frecuencia se considera vigente durante `ex_data` segundos desde la ultima
respuesta valida observada. Mientras sea vigente no se genera polling propio.

## Polling

Cuando no existe una frecuencia vigente y `polling` esta habilitado, se envia
`FA;` al K3. Si no llega respuesta, se reintenta una vez por segundo. Una
respuesta `IF` o `FA` valida cancela inmediatamente la necesidad de polling.

## Recuperacion

Si un puerto no puede abrirse o se desconecta, CAT Pacer mantiene el proceso
activo y reintenta la conexion cada dos segundos. No acumula una cola de datos:
solo conserva la frecuencia valida mas reciente.
