# sift

**Envía menos contexto. Mantén el original al alcance.**

sift comprime las salidas extensas de herramientas antes de enviarlas a un LLM. Reduce el uso de tokens y el coste de la caché de prompts, y permite recuperar desde un stash local el contenido original de cualquier compresión con pérdida. El motor está escrito en Rust y se ofrece para Node.js como [`@agent-context/sift`](npm/core/README.md).

[English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · Español

### **Un 62.3% menos de contexto. 14,129 tokens estimados ahorrados. Todos los benchmarks con pérdida recuperados.**

Los nueve escenarios incluidos pasan de **75,546 B a 28,447 B**, con recuperación correcta del original en **6/6 casos con pérdida**. [Consulta todos los resultados.](#cuánto-puede-ahorrar)

```sh
npm install @agent-context/sift
```

Estado: **Alpha** · Los detalles de la API pueden cambiar antes de 1.0 · [Notas operativas](#notas-operativas)

## Integraciones listas para usar

Si usas [Pi](https://github.com/earendil-works/pi) u [OpenCode](https://github.com/anomalyco/opencode), puedes instalar su adaptador directamente. Cada adaptador comprime automáticamente las nuevas salidas de herramientas y registra `sift_retrieve` para que el agente recupere el original del stash cuando lo necesite:

- **Pi:** `pi install npm:@agent-context/pi-sift`
- **OpenCode:** añade `["@agent-context/opencode-sift", { "minLength": 200 }]` al array `plugin` de `opencode.json`

Consulta [agents-sdk/sift-plugins](https://github.com/agents-sdk/sift-plugins) para ver la instalación, configuración, almacenamiento y resolución de problemas.

## ¿Por qué sift?

Las conversaciones de los agentes crecen rápidamente con logs de compilación, resultados de búsqueda, diffs, código fuente y respuestas JSON. Normalmente, solo una parte de esos datos es relevante para el siguiente razonamiento. Reenviar todo en cada turno consume tokens y desplaza contexto más importante.

sift ofrece:

- **Menor coste de contexto** — el conjunto de benchmarks incluido fue **un 62.3% más pequeño**, pasando de 75,546 a 28,447 bytes.
- **Los detalles útiles primero** — prioriza errores, trazas, comandos, coincidencias relevantes y contexto estructural.
- **Compresión recuperable** — antes de devolver un resultado con pérdida, guarda el original completo y añade un marcador `<<stash:HASH>>`.
- **Seguridad para la caché de prompts** — no modifica los mensajes situados antes o en un ancla `cache_control` de Anthropic.
- **Un único punto de integración** — detecta automáticamente Anthropic Messages, OpenAI Chat Completions y OpenAI Responses.
- **Núcleo en Rust y API sencilla** — la lógica compilada de compresión se utiliza mediante una pequeña interfaz para Node.js.

### Más útil que truncar, más seguro que un resumen sin retorno

| Método | Reconoce el contenido | Original recuperable | Protege el prefijo de Anthropic | Rechaza resultados sin ahorro |
| --- | :---: | :---: | :---: | :---: |
| Truncado directo | No | No | No necesariamente | No |
| Resumen con LLM | Parcial | Normalmente no | No necesariamente | No |
| **sift** | **Sí** | **Sí** | **Sí** | **Sí** |

## ¿Cuánto puede ahorrar?

Resultados medidos con las nueve [entradas de demo](npm/core/demo/cases) deterministas del repositorio y el paquete publicado `0.0.1-alpha.7`. Consulta [BENCHMARK.md](BENCHMARK.md) para ver la metodología y cómo reproducirlos:

| Escenario | Entrada | Salida | Reducción | Tokens ahorrados estimados | Recuperación |
| --- | ---: | ---: | ---: | ---: | --- |
| Array JSON | 18,397 B | 2,975 B | 83.8% | 4,627 | PASS |
| Pretty JSON | 3,642 B | 2,201 B | 39.6% | 432 | Sin pérdida |
| Log de compilación | 3,073 B | 1,543 B | 49.8% | 459 | Sin pérdida |
| Resultados de búsqueda | 10,057 B | 3,227 B | 67.9% | 2,049 | PASS |
| Git diff | 23,007 B | 12,759 B | 44.5% | 3,075 | PASS |
| Salida de comandos mixta | 9,240 B | 1,601 B | 82.7% | 2,291 | PASS |
| Código fuente Rust | 2,282 B | 402 B | 82.4% | 564 | PASS |
| Texto plano repetido | 2,723 B | 614 B | 77.5% | 632 | PASS |
| Protección de datos únicos y valores sensibles | 3,125 B | 3,125 B | 0% | 0 | Sin cambios |
| **Total** | **75,546 B** | **28,447 B** | **62.3%** | **14,129** | **6/6 casos con pérdida recuperados** |

Son resultados transparentes de fixtures públicos, no una promesa para cualquier carga de trabajo. El único caso sin cambios se incluye deliberadamente: la compresión conservadora de texto plano conserva los datos únicos. `tokensSaved` usa el estimador integrado de sift; el recuento real y el ahorro dependen del tokenizer del modelo y de tus datos.

## Inicio rápido

```sh
npm install @agent-context/sift
```

Comprime una solicitud justo antes de enviarla al modelo:

```ts
import OpenAI from "openai";
import { siftRequest } from "@agent-context/sift";

const openai = new OpenAI();
const request = {
  model: "gpt-5.6-sol",
  input: conversationWithLargeToolOutputs,
};

const result = siftRequest(request, currentUserQuestion);
const response = await openai.responses.create(result.body as any);

console.log({
  changed: result.changed,
  tokensSaved: result.tokensSaved,
  blocksCompressed: result.blocksCompressed,
});
```

`siftRequest` solo cambia las salidas de herramientas aptas. Los prompts de system, user y assistant están protegidos de forma predeterminada.

Para comprimir el resultado de una herramienta o un archivo por separado:

```ts
import { siftText } from "@agent-context/sift";

const result = siftText(
  fileContents,
  currentUserQuestion,
  "src/services/OrderService.java", // opcional: mejora la detección del lenguaje
);

console.log(result.text);
console.log(result.tokensSaved);
```

Las entradas menores de 512 bytes se devuelven sin cambios, por lo que puedes colocar sift en una ruta general sin filtrar previamente cada bloque.

### Lo que ve el modelo

A modo ilustrativo, en lugar de arrastrar cientos o miles de líneas repetitivas al siguiente turno, el modelo conserva la estructura útil y una ruta al original:

```diff
- 2,000 líneas de comandos, estados repetidos y trazas
+ $ cargo test --workspace
+ error[E0382]: borrow of moved value: `request`
+   --> src/client.rs:84:17
+ [... 1,962 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 19]
+ test result: FAILED. 127 passed; 1 failed
+ <<stash:HASH>>
```

El error y el resumen siguen visibles. Las líneas omitidas se recuperan mediante el marcador stash o el intervalo exacto del archivo.

## Recupera el original

La compresión con pérdida nunca devuelve el resultado hasta que la entrada completa se ha escrito en el stash. La salida incluye un marcador como este:

```text
<<stash:8f1c2e...>>
```

Puedes recuperar el original completo o solo las líneas necesarias:

```ts
import { retrieve, retrieveLines, siftText } from "@agent-context/sift";

const result = siftText(longToolOutput, currentUserQuestion);

if (result.stashKey) {
  const original = retrieve(result.stashKey);
  const slice = retrieveLines(result.stashKey, 120, 80);
}
```

En código fuente, logs, resultados de búsqueda, diffs y texto por líneas, los avisos de omisión pueden apuntar directamente al archivo del stash y al intervalo exacto:

```text
// ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32
```

Un agente que comparta el sistema de archivos puede leer ese intervalo directamente. En otros casos, ofrece `retrieve` o `retrieveLines` mediante tu propia herramienta o flujo. sift no inyecta automáticamente una herramienta de recuperación en el modelo.

## Compresión según el contenido

| Entrada | Qué conserva o simplifica sift |
| --- | --- |
| Arrays JSON | Esquema, muestras representativas y registros importantes o con errores |
| Logs de compilación y pruebas | Comandos, errores, trazas y resúmenes |
| Resultados de grep / ripgrep | Coincidencias útiles agrupadas con su contexto de código |
| Unified diffs | Hunks representativos y estructura de los cambios |
| Código fuente | Firmas y estructura, plegando cuerpos aptos; compatible con Python, JavaScript, TypeScript, Go, Rust, Java, C y C++ |
| Texto plano | Bloques exactamente duplicados dentro de la misma sección; los datos únicos permanecen visibles |
| JSON con formato y logs repetitivos | Minificación o plantillas sin pérdida cuando es suficiente |

Actualmente, HTML se devuelve sin cambios.

## Diseñado para una adopción segura

sift sigue tres reglas innegociables:

1. Comprime dentro de cada mensaje; nunca elimina mensajes completos de una conversación.
2. No modifica el prefijo congelado anterior o igual a un ancla `cache_control` de Anthropic.
3. Toda transformación con pérdida guarda el original antes de publicar el resultado comprimido.

Otras protecciones mantienen los pares de llamada/resultado de herramientas, las etiquetas XML personalizadas y las cadenas de alta entropía que podrían contener credenciales. Si la compresión no ahorra tokens o falla la escritura del stash, sift devuelve el contenido original.

## Dónde integrarlo

Ejecuta `siftRequest` como último middleware antes de una solicitud saliente al LLM. Resulta especialmente útil para:

- agentes de programación que conservan repetidamente compilaciones, búsquedas y diffs;
- asistentes de larga duración con respuestas extensas de herramientas;
- gateways que aceptan formatos de Anthropic y OpenAI;
- flujos locales o de servidor donde el modelo puede pedir más tarde los detalles omitidos.

Usa `siftText` cuando tengas una cadena sin un cuerpo de solicitud completo.

## Resumen de la API

```ts
siftRequest(body, query?)
siftText(text, query?, sourcePath?)
retrieve(key)
retrieveLines(key, startLine, lineCount)
createSift({ stashDir })
detectContentType(text)
detectRequestFormat(body)
```

Consulta la [documentación del paquete Node.js](npm/core/README.md) para conocer los tipos de retorno, los formatos y el comportamiento completo.

## Notas operativas

- El stash predeterminado es `~/.sift/stash`; usa `SIFT_STASH_DIR` o `createSift({ stashDir })` para elegir otro directorio.
- Las entradas caducan después de 30 minutos y se eliminan de forma diferida al leerlas. Diseña la recuperación y la retención teniendo esto en cuenta.
- Un stash local se comparte entre procesos de una máquina, pero no entre hosts de un clúster. Usa un sistema de archivos compartido o un backend `StashStore` compartido para despliegues multinodo.
- `tokensSaved` es una estimación para observabilidad, no para conciliar facturación.
- El paquete Node.js incluye binarios x64 y arm64 para macOS, Linux (GNU y musl) y Windows. Los binarios GNU de Linux usan glibc 2.28 como base.

## Contribuir

Las instrucciones de compilación, las reglas de arquitectura, los requisitos de pruebas y el proceso de publicación están en [CONTRIBUTING.md](CONTRIBUTING.md).

## Licencia

[Apache-2.0](LICENSE)
