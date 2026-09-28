# 08 — Transports

## 1. Modèle commun

Un transport est un adaptateur entre le **producteur de messages** et un support.

```
Encodeur ──▶ MessageBus ──▶ TransportAdapter ──▶ Support
                            (1 message)         (socket / port série)
```

Un `Message` est une structure, jamais une chaîne déjà concaténée :

```ts
type Message =
  | { kind: 'nmea';    lines: string[] }        // framing à la sortie
  | { kind: 'json';    payload: object }
  | { kind: 'csv';     fields: (string|number)[] }
  | { kind: 'raw';     text: string; eol: string | null };
```

C'est le `eol` explicite qui permet à un même message d'être envoyé en CRLF sur
TCP et sans terminateur sur le Signal K, sans que l'encodeur le sache.

## 2. Catalogue

| Type | Legacy (valeur numérique) | Cible (nom) | Support |
|---|---|---|---|
| WebSocket server | 0 | `websocket-server` | TCP + TLS optionnel |
| TCP server | 1 | `tcp-server` | TCP |
| TCP client | 2 | `tcp-client` | TCP |
| Port série | 3 | `serial` | série |
| UDP broadcast | 4 | `udp-broadcast` | UDP |
| UDP client | 5 | `udp-client` | UDP |
| UDP multicast | 6 | `udp-multicast` | UDP |

La migration accepte les valeurs numériques du legacy et les convertit en noms.
Un nom inconnu ou un nombre hors plage produit une **erreur de validation** avec
le champ fautif et la liste des valeurs acceptées. Le legacy, lui, ignore
silencieusement une chaîne et ne démarre aucun serveur.

## 3. Comportements hérités, à conserver

| Comportement | Détail | Statut |
|---|---|---|
| TCP server sur toutes les interfaces | `listen(port)` sans hôte | à conserver |
| NMEA TCP : un write par phrase | `\r\n` inclus dans chaque write | à conserver |
| Signal K : un write JSON | pas de CRLF | à conserver |
| Coalescing TCP | possible, non contractuel | documenté |
| UDP : un datagramme par phrase | port source éphémère | à conserver |
| Broadcast sur l'interface sélectionnée | adresse de diffusion dérivée de l'interface | à conserver |
| Hello Signal K à la connexion | TCP et WebSocket | à conserver |
| Aucun hello en NMEA | | à conserver |
| ViewSync sur son propre socket | port distinct, non partagé avec le serveur principal | à conserver |

## 4. Défauts de cycle de vie, à corriger

### 4.1 Résolution avant le bind

Le legacy fait résoudre la promesse de `startTCPServer()` avant le callback
`listen`. L'UI affiche alors « démarré » alors que le bind peut échouer.

Cible :

```ts
async start(): Promise<void> {
  if (this.state === 'running') return;
  this.state = 'starting';
  try {
    await new Promise<void>((resolve, reject) => {
      this.server.once('error', reject);
      this.server.listen(this.port, () => {
        this.server.removeListener('error', reject);
        resolve();
      });
    });
    this.state = 'running';
  } catch (e) {
    this.state = 'failed';
    throw new TransportError(this.id, e);
  }
}
```

L'UI ne lit que `state`, qui ne peut être `running` qu'après un bind réussi.

### 4.2 Fenêtre de course au changement de type

Le legacy ne bascule de type que si `this.server` est falsy, et `stop()` ne le
vide pas de façon synchrone. Résultat : changer de type juste après un arrêt peut
conserver l'ancien adaptateur.

Cible : `Multiplex` possède une map `id -> adapter`. `stopTransport(id)` retire
l'entrée de la map **avant** d'attendre la fermeture, et une opération de
changement de type est sérialisée par un verrou par `id`.

## 5. Multiplex

```ts
class Multiplex {
  private adapters = new Map<string, TransportAdapter>();
  private locks = new Map<string, Promise<void>>();

  async apply(spec: TransportSpec[]): Promise<void> {
    // diff : ajouter, modifier, retirer ; chaque id traité sous verrou
  }
}
```

Points de conception :

- **File bornée par transport.** Si le lecteur est lent, le transport concerné
  est marqué `degraded`, puis déconnecté si le retard dépasse `maxQueueDelay`. Le
  noyau n'est jamais bloqué par un socket.
- **Compteurs par transport** : messages, octets, erreurs, retard maximal,
  clients connectés, dernier horodatage d'envoi.
- **Aucune dépendance entre transports** : la panne d'un n'affecte ni la
  simulation ni les autres.

## 6. Détail par transport

### 6.1 TCP server

- Écoute sur toutes les interfaces, comme le legacy. `host` optionnel pour
  restreindre à une interface.
- Une connexion par client, sans limite dure mais avec plafond configurable.
- `setNoDelay(true)` : le legacy écrit toutes les secondes sans segmentation
  anormale ; Nagle n'apporte rien ici et ajoute de la latence.
- Backpressure : si le tampon d'un socket client dépasse le seuil, il est
  déconnecté proprement plutôt que de faire croître la file.

### 6.2 TCP client

- Connexion à `host:port`, reconnexion avec backoff exponentiel et jitter.
- Legacy vérifié (`legacy/cap-tcpclient*.log`, `legacy/console-notarget.log`) :
  flux correct, segments TCP non alignés sur les phrases, **aucune
  reconnexion**, et `started` émis avant la connexion, si bien que l'UI annonce
  un succès sur `ECONNREFUSED`. Les deux derniers points sont **FIX**.
- Le récepteur doit traiter un flux d'octets délimité par CRLF.
- La cible doit distinguer « connecté », « en reconnexion » et « échoué ».

### 6.3 UDP

- **broadcast** : `setBroadcast(true)`, envoi à l'adresse de diffusion de
  l'interface sélectionnée. Le port source est éphémère, comme observé.
- **client** : envoi unicast, port source éphémère, aucun bind.
- **multicast** : `addMembership(group, interface)`, `setMulticastTTL`,
  `setMulticastLoopback`. Legacy vérifié (`legacy/cap-mcast.log`) : deux
  récepteurs servis à l'identique, pas de contrôle de flux. L'émetteur legacy
  rejoint lui-même le groupe sur son port éphémère sans interface : inutile,
  non reproduit (**FIX**).
- **broadcast (legacy)** : la destination est l'adresse de diffusion dérivée du
  CIDR de l'interface ; `ip.address` est ignoré. Le mode client, lui, utilise
  `ip.address`.

Décision à acter : autoriser ou non le bind d'un port source fixe pour
UDP, utile pour les firewalls. Le legacy ne le fait pas ; par défaut, ne pas le
faire.

### 6.4 WebSocket

- Un serveur, un chemin configurable, une origine configurable.
- Un message WebSocket par message de sortie (NMEA : un message par phrase ;
  Signal K : un message par delta).
- Le legacy n'envoie pas de hello en NMEA : conservé.
- Pas de compression activée par défaut (elle dégrade le débit utile pour du
  texte court et répétitif).

### 6.5 Série

- Réglages : `baudRate`, `dataBits`, `stopBits`, `parity`, `flowControl`.
- Détection et reconnexion automatique sur débranchement, avec debounce.
- Legacy vérifié sur paire de PTY (`legacy-p2/cap-serial.log`) : un write par
  phrase, CRLF, 408 phrases valides en 17 cycles. Un chemin absent de la liste
  détectée exige `manualSerialPort: true`. Aucune reconnexion dans le code.
- La page Settings du legacy peut réécrire le type série en WebSocket quand
  aucun port n'est détecté (analyse statique, `mods_b/7238.js:395`) : la cible
  ne réécrit jamais un type implicitement.
- Deux directions : la cible doit gérer l'**entrée** série, qui est une source
  d'instructions (`override`, `setDestination`) et non du bruit. Le legacy
  lit l'entrée ligne à ligne puis se contente d'un `console.info` (vérifié).

## 7. Sécurité réseau

| Point | Règle |
|---|---|
| Écoute distante | refusée par défaut sur TCP server et WebSocket server, sauf `allowRemote: true` |
| API de contrôle | localhost par défaut, jeton optionnel, pas de découverte réseau |
| TLS | supporté pour WebSocket et TCP, via certificats fournis par l'utilisateur |
| Ports | < 1024 refusés sauf élévation explicite |
| Journalisation | aucun contenu de message journalisé au niveau `info` |

## 8. Observabilité

Chaque transport expose :

```ts
interface TransportStatus {
  id: string;
  type: string;
  state: 'stopped' | 'starting' | 'running' | 'degraded' | 'failed';
  endpoint?: string;
  clients?: number;
  sent: { messages: number; bytes: number };
  errors: { count: number; last?: { message: string; at: number } };
  queueDelayMs: number;
  lastSentAt?: number;
}
```

Ces données alimentent un panneau d'état dans l'UI et une métrique exposée pour
les bancs d'essai.
