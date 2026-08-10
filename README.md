# Creatura semiliquida

Primo prototipo vettoriale di una creatura 2D deformabile. Il corpo è formato
da 64 punti esterni e quattro moduli interni deformabili. Ogni modulo contiene
un anello di 8 punti e un nucleo triangolare di 3 punti; collegamenti morbidi
uniscono i moduli vicini. Ogni zona della membrana dipende dai due moduli più
vicini, evitando giunzioni rigide tra settori. La
visualizzazione è volutamente diagnostica: mostra punti e collegamenti, senza
immagini o animazioni preregistrate.

## Avvio

Con `uv`:

```bash
uv sync --extra dev
uv run creatura-demo
```

In alternativa, con Python:

```bash
python -m venv .venv
.venv/bin/pip install -e '.[dev]'
.venv/bin/creatura-demo
```

Trascinare un punto con il tasto sinistro per deformare il corpo. Con il tasto
destro si seleziona una zona della membrana; tenendo premuta la barra spaziatrice
si estende gradualmente uno pseudopodio, che si ritrae quando la barra viene
rilasciata. Premendo `Invio` viene eseguito un passo locomotorio completo:
estensione, ancoraggio della punta, trazione, rilascio. `R` ripristina la forma
iniziale, `Esc` chiude la demo.

Un click destro lontano dalla membrana imposta invece un bersaglio: la creatura
sceglie automaticamente il fronte corretto e ripete i passi fino a raggiungerlo.
`S` interrompe la navigazione continua.

La demo include ora una strettoia intermedia larga 130 px fra due pareti solide.
La creatura parte a sinistra e il cibo si trova oltre il passaggio: un click destro sul lato destro
permette di provare l'avvicinamento. I contatti vengono risolti sia sui nodi sia
sui segmenti della membrana, applicano attrito tangenziale e attivano
automaticamente il raffinamento locale. In questa prima versione le pareti e le
collisioni sono operative; l'adattamento autonomo della rigidità e la scelta
intelligente dell'apertura saranno aggiunti nella fase successiva.

Premendo `M` si può confrontare la struttura a quattro moduli con un secondo
modello sperimentale. Quest'ultimo mantiene la stessa membrana da 64 punti ma la
sostiene con una rete triangolare distribuita, collegata alla membrana attraverso
tre vincoli per zona. Bersaglio, strettoia, pseudopodi e collisioni rimangono
identici, rendendo il confronto diretto.

## Modello a nuvola dinamica

Un terzo prototipo elimina completamente molle, ancoraggi e membrana fisica. La
creatura è una nuvola variabile di punti: il vicinato viene ricalcolato, punti
troppo vicini possono fondersi e nuove particelle vengono inserite nelle zone
rade. I punti generano un campo di densità continuo in stile metaball; marching
squares ne estrae una linea di livello, poi ricampionata e filtrata nel tempo per
ottenere una curva vettoriale stabile.

```bash
uv run --no-sync python -m liquid_creature.cloud_demo
```

Il click destro imposta il bersaglio. Ogni ciclo genera due pseudopodi frontali
(periodicamente tre), trasferisce poi la massa centrale e richiama infine il
retro. Vicino alle pareti i punti rallentano e aderiscono mediante un'attrazione
locale, senza ancoraggi permanenti. Il restringimento laterale viene applicato
soltanto in presenza di superfici e con intensità limitata. `R` ricrea la nuvola.
Ogni punto trasporta una quantità di massa: le fusioni sommano i pesi e le nuove
particelle ricevono massa dai vicini. La soglia metaball si adatta all'area di
riferimento e, sotto compressione, la larghezza persa viene redistribuita lungo
il passaggio invece di far "sgonfiare" la superficie.

Durante la navigazione, ogni ciclo introduce variazioni controllate nella
direzione locale, nella larghezza e lunghezza dello pseudopodio, nei tempi di
estensione e ritrazione e nell'ampiezza della zona di adesione. La casualità non
è un'animazione: modifica realmente i parametri fisici del passo.

## Cibo e contatti

Il cerchio arancione è il primo oggetto-cibo fisico. Può essere trascinato con
il tasto sinistro e respinge i punti della membrana, che non possono
attraversarlo. Un click destro sul cibo avvia la fagocitosi: la creatura si
avvicina, forma due lembi nella zona di contatto, avvolge l'oggetto e lo
assorbe. Dopo l'ingestione l'area di equilibrio aumenta del 4%.
L'avvolgimento parte dalla posizione reale di ciascun punto: i nodi non
collassano più immediatamente verso il centro del contatto. I due lembi avanzano
seguendo l'ordine topologico della membrana, senza incrociarsi. Dopo la chiusura
il cibo resta nella tasca appena formata e viene digerito sul posto: non viene
più trascinato cinematicamente verso il centro, evitando la spinta di reazione
sull'intero corpo.
La sequenza può passare all'internalizzazione soltanto quando il centro del cibo
risulta geometricamente dentro il contorno. Dopo la chiusura la preda e la tasca
anteriore restano ferme nello spazio. La struttura interna avanza verso di loro
con velocità limitata a 32 px/s e i collegamenti radiali trascinano il resto del
corpo in avanti. La digestione inizia soltanto quando il centro interno della
creatura raggiunge la preda.
L'anello interno segue bersagli XPBD soffici, non una traslazione cinematica:
questi bersagli avanzano fino alla posizione fissa della preda e attendono che
la struttura reale li raggiunga, contrastando il ritorno elastico dei raggi.
Durante l'assorbimento la tasca mantiene una forza morbida costante, così non si
apre mentre il cibo è ancora visibile. Dopo la scomparsa inizia una fase separata
di rilassamento: la forza decade quadraticamente per circa 1,8 secondi e soltanto dopo
i bersagli vengono rimossi. Il rilascio non produce quindi un contraccolpo.
La selezione del cibo ha priorità su qualsiasi bersaglio precedente: interrompe
il passo in corso, libera la vecchia adesione e avvia immediatamente
l'avvicinamento fagocitario.
Nella demo si può avviare la sequenza sia con il click destro sul cibo sia con
`F`, che seleziona automaticamente il cibo non consumato più vicino. Il cibo
iniziale è collocato più vicino alla creatura per rendere rapidamente visibili
le fasi di avvolgimento e digestione.

Per impostazione predefinita la fagocitosi è automatica. Quando una preda entra
nel raggio sensoriale di 90 px dalla membrana, il segnale di adesione cresce in
modo continuo; l'anello giallo intorno al cibo mostra l'accumulo. Superata la
soglia del 62%, la creatura interrompe altri bersagli e avvia autonomamente la
cattura. Fuori dal raggio il segnale decade. `S` disattiva l'automatismo, mentre
`F` lo riattiva e forza la cattura del cibo più vicino.
Al termine della digestione l'aumento di area viene distribuito nel tempo e una
breve fase viscosa conserva il centro mentre membrana e struttura interna
scaricano la tensione residua. Un nuovo passo locomotorio interrompe subito
questo assestamento, quindi la creatura non rimane artificialmente bloccata.
Durante la chiusura una piccola zona posteriore aderisce temporaneamente al
substrato: questo impedisce al corpo di orbitare intorno al cibo per effetto
della trazione tangenziale esercitata dai lembi anteriori.

La risoluzione resta configurabile: `SoftBody.create(outer_count=48,
inner_count=12)` permette di confrontare un corpo ancora più leggero con quello
predefinito a 64/16 punti.

## Raffinamento adattivo

Vicino al cibo e agli pseudopodi il solver può inserire fino a 64 nodi fisici
temporanei. Ogni nodo sostituisce localmente un segmento della membrana con due
segmenti più piccoli, partecipa alle collisioni ed è mostrato in arancione. Se
la zona non richiede più dettaglio, il nodo viene escluso dalla topologia attiva
e il segmento originale viene ripristinato. Gli indici della struttura interna
rimangono così stabili.
Il raffinamento si attiva anche quando un segmento è molto allungato o forma una
curvatura stretta, senza richiedere un cibo o uno pseudopodio nelle vicinanze.

Tre onde corticali lente modificano continuamente le distanze radiali di
equilibrio. Durante il moto si aggiunge una contrazione irregolare sul lato
opposto allo pseudopodio principale: il retro non forma più un semicerchio
perfetto e la sagoma mantiene rigonfiamenti e rientranze persistenti.
Le stesse onde modulano anche la lunghezza locale dei segmenti della membrana;
il vincolo di curvatura e l'anello interno sono più morbidi, quindi queste
variazioni non vengono più filtrate in un arco regolare. La zona posteriore usa
due lobi sfalsati con tensioni differenti invece di una contrazione centrale
simmetrica.
La geometria iniziale usa già la stessa combinazione di armoniche del cortex:
la creatura non attraversa più una transizione visibile da cerchio perfetto ad
ameba nei primi aggiornamenti.

L'adesione locomotoria non viene più rimossa tutta nello stesso istante. I punti
si staccano a coppie dai bordi verso il centro della zona aderente; al termine
la velocità interna residua viene smorzata, evitando lo scatto e la vibrazione
che seguivano il rilascio simultaneo.

Durante l'avvolgimento i punti della coppa non sono più bloccati su posizioni
rigide. Seguono attrattori elastici con risposta e velocità massima limitate;
anche il campione precedente del Verlet viene spostato, così la deformazione non
inietta un impulso artificiale. Rimane ancorata soltanto una piccola zona
posteriore.

Durante la locomozione vengono inoltre generate da una a tre protrusioni
secondarie temporanee, più corte e deboli del fronte principale. Nascono e
scompaiono con un inviluppo morbido e richiamano localmente ulteriori nodi
adattivi. La membrana recupera gradualmente la propria tensione quando questi
nodi vengono rimossi, evitando un contraccolpo al termine della fagocitosi.

## Test

```bash
uv run pytest
```

Questa fase verifica stabilità, deformazione manuale, recupero della forma e un
primo pseudopodio fisico controllato manualmente. Collisioni e locomozione
procedurale saranno aggiunte sopra questa base dopo la verifica visiva.

## Banco di prova PBF

Il quarto modello è un solver Position Based Fluids indipendente dai prototipi
precedenti. Usa particelle non collegate, una griglia spaziale per i vicini e un
vincolo iterativo di densità per contrastare la compressione. La prima scena
verifica soltanto stabilità, massa costante e collisioni nella strettoia:

Il corpo usa inoltre tre livelli complementari: il PBF conserva la massa, una
membrana virtuale limita la crescita del perimetro senza molle permanenti e un
nucleo ellittico deformabile conserva la propria area. Il nucleo ha collisioni
proprie e stabilisce una dimensione minima proporzionale alla creatura, quindi
il limite delle strettoie non dipende dalla risoluzione delle particelle.

```bash
uv run creatura-pbf-demo
```

La demo si apre nella prima stanza di prova. Tieni premuto il tasto destro del
mouse per guidare la creatura e usa `Spazio` per uno scatto temporaneo. Lo
scatto consuma energia; i nutrienti la recuperano, mentre la zona rosa la
consuma. Dopo avere raccolto i tre nutrienti si sblocca l'uscita verde.

Controlli aggiuntivi:

- `L`: ricomincia la stanza;
- `C`: apre il laboratorio con tre aperture di ampiezza diversa;
- tasto sinistro: estende localmente uno pseudopodio verso il cursore; la
  portata e la massa coinvolta sono limitate in proporzione alla taglia;
- `1`-`4`: prove in strettoie di larghezza diversa;
- `P`, `M`, `G`: taglia piccola, media o grande;
- `F1`, `F2`, `F3`: punti, membrana o entrambe;
- `R`: riavvia la prova corrente.
