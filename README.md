# Verdigris: Motor de Decisão Coletiva

> **Disciplina:** Engenharia de Software (ENG-SFT)
>
> **Autor:** Felipe Leme de Argollo Ferrão (RA: 260431)
>
> **Formato:** Desenvolvimento individual (Solo), com foco em backend, qualidade e confiabilidade.

## 1. Produto e Contexto

O **Verdigris** é um motor de decisão coletiva que oferece um núcleo comum para diferentes processos de votação, separando as regras específicas de contexto da infraestrutura de submissão, integridade e apuração. O projeto alinha-se ao **ODS 16 da ONU**, com ênfase em transparência, auditabilidade e resistência a adulterações.

Esta primeira versão trata-se de um **protótipo acadêmico (MVP)** para demonstrar de forma funcional como um único núcleo pode atender a múltiplos contextos de forma extensível.

### Contextos de Decisão Suportados

1. **Centro Acadêmico:** Eleitor elegível; 1 voto por estudante; chapas oficiais; quórum mínimo. *(Apuração: contagem simples)*.

2. **Assembleia de Condomínio:** Condômino elegível; peso do voto pela fração ideal; quórum qualificado. *(Apuração: soma ponderada)*.

3. **Orçamento Participativo:** Cidadão elegível; propostas; preferência ranqueada por Borda; teto orçamentário. *(Apuração: pontuação de preferência + seleção dentro do teto)*.

### Pressupostos e Limites do MVP

* **Núcleo Comum:** Estratégias de apuração sobre uma base única, evitando três aplicações independentes.

* **Armazenamento:** Append-only no nível lógico do domínio (registros de eventos e evidências).

* **Sigilo:** Separando a emissão de elegibilidade da submissão do voto (sem promessa de anonimato absoluto de nível de produção nesta fase).

* **Interface:** API HTTP/REST acompanhada de um frontend minimalista para demonstração de fluxos.

* **Fora do escopo inicial:** Assinaturas cegas completas, chaves de limiar e criptografia de produção avançada.

## 2. Processo de Desenvolvimento

O projeto adota uma abordagem de **Kanban Contínuo** adaptada para equipe solo, combinando Modelagem Orientada ao Domínio (DDD), TDD e Integração Contínua (CI/CD).

### Fluxo de Trabalho (Pipeline do Processo)

1. **Backlog de Domínio:** Registro de necessidades, ambiguidades e riscos.

2. **Modelagem de Domínio e Regras:** Definição de participantes, estados e restrições (atualizando classes, traits e exemplos).

3. **Especificação + TDD:** Conversão de critérios de aceitação em testes executáveis em Rust (`cargo test`) antes do código principal.

4. **Implementação do Núcleo E2E:** Codificação em Rust do núcleo comum e da estratégia específica do contexto.

5. **CI/CD e Portão de Qualidade:** Validação automática de compilação, testes, análise estática e auditoria de dependências via GitHub Actions.

6. **Revisão e Validação:** Verificação de arquitetura, separação de identidade/conteúdo e cenários do domínio.

7. **Release / Demonstração:** Disponibilização de features aceitas.

## 3. Roadmap de Implementação

* **Fase 1 - Núcleo e Domínio:** Configuração do projeto em Rust, definição de traits comuns, fluxo básico de voto e implementação do primeiro contexto (Centro Acadêmico).

* **Fase 2 - Integridade e Auditoria:** Adição de hashing/commit, registro append-only lógico, recibo de auditoria e árvore de Merkle.

* **Fase 3 - Estratégias Adicionais:** Inclusão do Condomínio (peso por fração ideal) e Orçamento Participativo (Borda + teto).

* **Fase 4 - API e Identidade:** Exposição de endpoints HTTP/REST com separação entre a emissão de elegibilidade e o voto anônimo.

* **Fase 5 - Qualidade e Demonstração:** Consolidação de CI/CD, testes de integração, documentação técnica e demonstração end-to-end.

## 4. Artefatos Principais do Processo

| Artefato | Finalidade |
| ----- | ----- |
| **Backlog de Domínio** | Registrar necessidades, mudanças de regra, riscos e aprendizados. |
| **Modelo de Domínio e Traits** | Representar conceitos comuns e regras específicas dos três contextos. |
| **Suíte de Testes (TDD)** | Definir e verificar comportamentos esperados de apuração e elegibilidade. |
| **Núcleo E2E + API** | Implementar submissão, persistência lógica e evidências. |
| **Checklist de Sigilo/Arquitetura** | Garantir a separação entre identidade e conteúdo, e extensibilidade. |
| **Relatórios de CI** | Registrar resultados de build, testes e análise estática. |

## 5. API HTTP

Inicie o servidor local:

```sh
cargo run
```

Ele escuta em `http://localhost:3000` e oferece estes endpoints:

| Método | Caminho | Descrição |
| ----- | ----- | ----- |
| `POST` | `/api/elections` | Cria uma eleição a partir de um manifesto JSON e retorna seu `id`. |
| `POST` | `/api/elections/{id}/votes` | Registra um voto JSON para a eleição. |
| `POST` | `/api/elections/{id}/close` | Fecha a eleição e retorna o resultado JSON. |

O corpo de `POST /api/elections` tem o mesmo formato de manifesto descrito
abaixo. Cada voto deve ser JSON no formato esperado pelo tipo da eleição; por
exemplo, uma eleição acadêmica recebe uma string como `"A"`. O endpoint de
fechamento consome a eleição, portanto ela não poderá receber mais votos nem
ser fechada novamente. Erros da API retornam um status HTTP apropriado e um
objeto JSON com a mensagem em `error`.

Exemplo criando e votando em uma eleição acadêmica:

```sh
curl -X POST http://localhost:3000/api/elections \
  -H 'content-type: application/json' \
  -d '{"type":"academic","slates":["A","B"],"minimum_quorum":1,"votes":[]}'

curl -X POST http://localhost:3000/api/elections/ID/votes \
  -H 'content-type: application/json' \
  -d '"A"'

curl -X POST http://localhost:3000/api/elections/ID/close
```

Substitua `ID` pelo valor retornado pela primeira chamada.

Para executar o fluxo anterior de linha de comando e apurar uma configuração
localmente sem iniciar o servidor, passe o caminho do manifesto:

```sh
cargo run -- caminho/para/config.json
```

O manifesto seleciona o tipo de eleição com o campo `type`: `academic`,
`budget`, `condo` ou `custom`. Os três tipos padrão recebem suas definições e
votos diretamente no manifesto. Por exemplo:

```json
{
  "type": "academic",
  "slates": ["A", "B"],
  "minimum_quorum": 2,
  "votes": ["A", "B"]
}
```

Uma eleição `budget` usa `projects` (cada item com `name` e `cost`),
`budget_ceiling` e votos ranqueados; `condo` usa `options`,
`minimum_fraction_quorum` e votos com `choice` e `fraction`. Valores decimais
são strings para preservar precisão. A factory retorna uma eleição atrás de
`Box<dyn ElectionBoundary>`. Assim, chamadores como handlers HTTP podem passar
um payload JSON para `cast_vote` e receber o resultado JSON de `close` sem
conhecer o tipo concreto. Erros de conversão JSON e de validação são retornados
como `String`.

Para `custom`, o manifesto contém `rules` (opções, método de pontuação,
ponderação, quórum, desempate e número de vencedores) e uma lista `votes` de
cédulas. As formas personalizadas atuais, sem o campo `type`, continuam sendo
aceitas. Os tipos discriminados das regras customizadas usam o campo `type`;
`src/config.json` serve como exemplo completo.

## 6. Schema da configuração

Gere o JSON Schema do manifesto (eleições padrão e customizadas) com descrições
e dicas para editores que suportam schema JSON:

```sh
cargo run --bin generate_schema
```

Por padrão, o comando atualiza `src/config.schema.json`. Um caminho de saída
alternativo pode ser passado como argumento. Os campos decimais são descritos
como strings, evitando a perda de precisão que pode ocorrer ao representar
valores decimais como números JSON.
