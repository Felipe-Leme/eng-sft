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
