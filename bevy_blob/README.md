# Prototipo Bevy del blob

Prima migrazione giocabile del progetto verso Bevy 0.19. Il corpo usa una
membrana a particelle con integrazione Verlet, vincoli di distanza, conservazione
dell'area e collisioni contro un livello verticale. Avian è già integrato per i
futuri oggetti rigidi e le query del mondo; il comportamento interno del blob
resta un solver dedicato.

La creatura usa una scala fisica globale del 65%. La stessa scala viene
propagata a collisioni, nucleo, indicatori e alle creature ottenute dalla
divisione.

## Avvio

```bash
cd bevy_blob
cargo run
```

## Comandi

- `A`/`D` oppure frecce: movimento laterale.
- Tieni `Freccia giù`: comprimi e carica il blob.
- Rilascia `Freccia giù`: salta; la durata della carica determina l'impulso.
- `R`: dopo la divisione attiva il ricongiungimento; con un solo blob effettua il reset.
- `X`: divide il blob in due creature indipendenti.
- `Tab`: seleziona alternativamente una delle due creature.
- `Esc`: chiude il gioco.

## Prossimi passi

1. sostituire la visualizzazione diagnostica con una mesh piena interpolata;
2. aggiungere contatti Avian accurati per pareti e piattaforme mobili;
3. introdurre ancoraggio e allungamento controllato;
4. rifinire visivamente la transizione di divisione e fusione già funzionante.
