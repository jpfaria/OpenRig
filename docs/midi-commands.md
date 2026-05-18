# MIDI — referência completa de comandos (#22)

Princípio: **tudo no OpenRig é controlável por MIDI.** Não existe lista
privilegiada. O mapa (`midi-map.yaml`) liga uma **fonte** MIDI (nota, CC,
program change) a um **comando** pelo nome + seus argumentos. É validado
contra a mesma fonte única de schema que o MCP usa — um comando só, em
qualquer lugar.

- Botão/footswitch → fonte discreta (`note_on`, `note_off`,
  `program_change`): dispara um comando fixo.
- Knob/fader → fonte contínua (`cc`) com `scale`: o valor 0–127 vira o
  valor do parâmetro (controle ao vivo).

Abaixo, **todos os 33 comandos**. "Argumentos" é o objeto `args:` do
binding. Comandos marcados **(novo #22)** foram adicionados para
controle ao vivo.

## Presets, cenas e seleção de blocos — controle ao vivo

| O que faz | `command` | `args` |
|---|---|---|
| Próximo preset (dá a volta) **(novo #22)** | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepPreset: 1 } }` |
| Preset anterior **(novo #22)** | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepPreset: -1 } }` |
| Próxima cena (dá a volta) **(novo #22)** | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepScene: 1 } }` |
| Cena anterior **(novo #22)** | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepScene: -1 } }` |
| Ir para um preset fixo (posição) | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { Preset: <n> } }` |
| Ir para uma cena fixa | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { Scene: <n> } }` |
| Mover a seleção de blocos (par) à frente **(novo #22)** | `SelectChainBlock` | `{ chain: "rig:<input>", delta: 2 }` |
| Mover a seleção de blocos atrás **(novo #22)** | `SelectChainBlock` | `{ chain: "rig:<input>", delta: -2 }` |
| Liga/desliga o bloco da esquerda do par **(novo #22)** | `ToggleSelectedBlock` | `{ chain: "rig:<input>", side: Left }` |
| Liga/desliga o bloco da direita do par **(novo #22)** | `ToggleSelectedBlock` | `{ chain: "rig:<input>", side: Right }` |

Preset/cena **dão a volta** (depois do último vem o primeiro). A
seleção de blocos é um **par** de dois blocos vizinhos; a linha em volta
aparece quando você manda o comando pelo MIDI e **some sozinha** depois
de uns segundos (pista passageira, não seleção fixa).

## Ligar/desligar e volume

| O que faz | `command` | `args` |
|---|---|---|
| Liga/desliga uma chain inteira | `ToggleChainEnabled` | `{ chain: "<chain-id>" }` |
| Liga/desliga um bloco específico | `ToggleBlockEnabled` | `{ chain: "<chain-id>", block: "<block-id>" }` |
| Define o volume de uma chain (valor fixo, %) | `SetChainVolume` | `{ chain: "<chain-id>", value: 80.0 }` |
| Sobe/desce volume com knob (0–127 → faixa) | `SetChainVolume` | + `scale: { min: 0, max: 200, into: value }` |

## Parâmetro de bloco (perfeito para knob/fader)

| O que faz | `command` | `args` |
|---|---|---|
| Define um parâmetro numérico (ganho, mix…) | `SetBlockParameterNumber` | `{ chain, block, path: "<param>" }` + `scale:` |
| Define um parâmetro liga/desliga | `SetBlockParameterBool` | `{ chain, block, path, value: true }` |
| Define um parâmetro de texto | `SetBlockParameterText` | `{ chain, block, path, value: "<txt>" }` |
| Escolhe uma opção de lista | `SelectBlockParameterOption` | `{ chain, block, path, value, index }` |
| Aponta um arquivo para o parâmetro | `PickBlockParameterFile` | `{ chain, block, path, file: "<caminho>" }` |
| Troca o modelo do bloco | `ReplaceBlockModel` | `{ chain, block, model_id: "<id>" }` |

`scale` (só fontes `cc`) mapeia 0–127 linearmente em `[min, max]` e
escreve no argumento `into` (padrão `value`).

## Edição de blocos e chains (possível por MIDI, mas é coisa de edição)

| O que faz | `command` | `args` |
|---|---|---|
| Adiciona um bloco | `AddBlock` | `{ chain, kind, model_id, position }` |
| Insere um bloco já montado | `InsertPrebuiltBlock` | `{ chain, block: <objeto>, position }` |
| Sobrescreve um bloco | `OverwriteBlock` | `{ chain, block, replacement: <objeto> }` |
| Remove um bloco | `RemoveBlock` | `{ chain, block }` |
| Move um bloco de posição | `MoveBlock` | `{ chain, block, new_position }` |
| Salva o loop de insert de um bloco | `SaveInsertBlock` | `{ chain, block, send, return_ }` |
| Adiciona uma chain | `AddChain` | `{ chain: <objeto Chain> }` |
| Configura uma chain | `ConfigureChain` | `{ chain: <objeto Chain> }` |
| Salva uma chain | `SaveChain` | `{ chain: <objeto Chain> }` |
| Remove uma chain | `RemoveChain` | `{ chain: "<chain-id>" }` |
| Move a chain para cima | `MoveChainUp` | `{ chain: "<chain-id>" }` |
| Move a chain para baixo | `MoveChainDown` | `{ chain: "<chain-id>" }` |
| Salva as entradas da chain | `SaveChainInputEndpoints` | `{ chain, input_blocks: [...] }` |
| Salva as saídas da chain | `SaveChainOutputEndpoints` | `{ chain, output_blocks: [...] }` |
| Salva entrada+saída da chain | `SaveChainIo` | `{ chain, input_block, output_block }` |
| Carrega um preset numa chain | `LoadChainPreset` | `{ chain, preset_blocks: [...] }` |

> Os que recebem objetos inteiros (`Chain`, bloco montado, lista de
> blocos) são tecnicamente disparáveis por MIDI, mas não são feitos para
> escrever à mão num mapa — são ações do editor.

## Projeto e áudio

| O que faz | `command` | `args` |
|---|---|---|
| Salva o projeto | `SaveProject` | *(nenhum)* |
| Carrega um projeto | `LoadProject` | `{ project: <objeto>, path: "<caminho>" }` |
| Cria um projeto novo | `CreateProject` | `{ project: <objeto> }` |
| Renomeia o projeto | `UpdateProjectName` | `{ name: "<nome>" }` |
| Salva config de áudio | `SaveAudioSettings` | `{ device_settings: [...] }` |

---

## Comportamento garantido (#22)

- **Footswitch mexe a tela e o som igual ao mouse.** Um comando vindo
  do MIDI atualiza a interface e o áudio ao vivo pelo mesmo caminho que
  um clique. (feito)
- **Preset/cena por footswitch:** avançar/voltar com volta no fim.
  (feito)
- **Seleção de blocos por footswitch:** par de dois, anda de dois em
  dois, liga/desliga esquerda/direita. (feito — falta só desenhar a
  linha na tela)

## O que ainda falta de código

1. **A linha em volta do par de blocos** — desenhar na tela, aparecendo
   no estímulo MIDI e sumindo sozinha depois de um tempo. A lógica de
   qual par está marcado já existe e é testada; falta o desenho + o
   timer de fade.
2. **Vários Chocolates ao mesmo tempo** — hoje o adapter abre **uma**
   porta MIDI. Precisa abrir todas as que casam (1, 2, 3 ou 4
   aparelhos), todas no mesmo caminho de comando. A separação entre
   pedais idênticos é por canal MIDI (configurável no Chocolate Plus).

Ambos serão feitos **TDD red-first**: teste que falha antes, depois o
código; onde for pixel de tela, o teste cobre a lógica que decide o que
a tela mostra (qual par, quando some).

## Como configurar o Chocolate (resumo)

1. **No pedal (app CubeSuite):** cada footswitch manda uma mensagem
   MIDI distinta — o mais simples: tipo **Note**, canal **1**, notas
   **60/61/62/63**. No Chocolate **Plus** dá para definir o canal por
   mensagem (permite vários pedais/bancos em canais diferentes).
2. **No OpenRig:** um `midi-map.yaml` ligando cada nota a um comando
   desta referência. Rode com `--midi` (veja `docs/midi.md`).

O pedal só manda números; o significado mora no `midi-map.yaml` — muda
quando quiser sem mexer no pedal.
