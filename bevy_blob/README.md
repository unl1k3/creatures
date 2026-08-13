# Prototipo Bevy del blob

Prima migrazione giocabile del progetto verso Bevy 0.19. Il corpo usa una
membrana a particelle con integrazione Verlet, vincoli di distanza, conservazione
dell'area e collisioni contro un livello verticale. Avian è già integrato per i
futuri oggetti rigidi e le query del mondo; il comportamento interno del blob
resta un solver dedicato.

## Avvio

```bash
cd bevy_blob
cargo run
```

## Comandi

- `A`/`D` oppure frecce: movimento laterale.
- Tieni `Freccia giù`: comprimi e carica il blob.
- Rilascia `Freccia giù`: salta; la durata della carica determina l'impulso.
- `R`: riporta il blob all'inizio e azzera il movimento.
- `Esc`: chiude il gioco.

## Prossimi passi

1. sostituire la visualizzazione diagnostica con una mesh piena interpolata;
2. aggiungere contatti Avian accurati per pareti e piattaforme mobili;
3. introdurre ancoraggio e allungamento controllato;
4. implementare divisione e fusione conservando massa e quantità di moto.
