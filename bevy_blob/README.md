# Prototipo Bevy del blob

Prima migrazione giocabile del progetto verso Bevy 0.19. Il corpo usa una
membrana a particelle con integrazione Verlet, vincoli di distanza, conservazione
dell'area e collisioni contro un livello verticale. Avian è già integrato per i
futuri oggetti rigidi e le query del mondo; il comportamento interno del blob
resta un solver dedicato.

La creatura usa una scala fisica globale del 65%. La stessa scala viene
propagata a collisioni, nucleo, indicatori e alle creature ottenute dalla
divisione.

Durante la divisione la risoluzione della membrana aumenta mantenendo invariate
area, massa e dimensioni. Una creatura selezionata può essere divisa nuovamente,
fino a un massimo di quattro parti attive. Ogni parte conserva la propria
genealogia: il ricongiungimento fonde prima i fratelli e risale progressivamente
fino alla creatura originale. Un tentativo che non raggiunge il contatto entro
quattro secondi viene annullato automaticamente. La divisione a cascata è
consentita soltanto ai frammenti con almeno 16 punti fisici.

## Avvio

```bash
cd bevy_blob
cargo run
```

## Comandi

- `A`/`D` oppure frecce: movimento laterale.
- Tieni `Freccia giù`: comprimi e carica il blob.
- Rilascia `Freccia giù`: salta; la durata della carica determina l'impulso.
- `R`: richiama il fratello della creatura selezionata; con un solo blob effettua il reset.
- `X`: divide la creatura selezionata, fino a quattro parti attive.
- `Tab`: passa alla creatura attiva successiva.
- `Esc`: chiude il gioco.

## Struttura del codice

- `main.rs`: stato del mondo, simulazione e ricongiungimento;
- `blob.rs`: modello e solver fisico della creatura;
- `input.rs`: comandi, selezione, divisione e reset;
- `camera.rs`: inseguimento della creatura selezionata;
- `rendering.rs`: piattaforme, contorni, colori familiari e indicatori;
- `blob_tests.rs`: prove dedicate alla fisica della membrana;
- `game_tests.rs`: prove di divisione, fusione, collisioni e camera.

## Prossimi passi

1. sostituire la visualizzazione diagnostica con una mesh piena interpolata;
2. aggiungere contatti Avian accurati per pareti e piattaforme mobili;
3. introdurre ancoraggio e allungamento controllato;
4. rifinire visivamente la transizione di divisione e fusione già funzionante.
