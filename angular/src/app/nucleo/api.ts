/**
 * El único punto por el que la interfaz habla con el servidor.
 *
 * `withCredentials: true` en todas las peticiones, y ni un token por ninguna
 * parte: la sesión es una cookie `HttpOnly` que este código **no puede leer**.
 * Es deliberado — un JWT en `localStorage` se lo lleva cualquier XSS, y como el
 * binario sirve esta misma interfaz no hay segundo origen que obligue a usarlo.
 */
import { HttpClient, HttpErrorResponse } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable, throwError } from 'rxjs';
import { catchError } from 'rxjs/operators';

/** Los papeles del sistema, tal como los devuelve el servidor. */
export type Rol = 'Admin' | 'Operador' | 'Supervisor' | 'Tercero';

export interface Actor {
  id: number;
  nombre: string;
  email: string;
  rol: Rol;
  stakeholder_id: number | null;
}

export interface Entrega {
  id: number;
  informe: string;
  /** El día que fija la norma. No se mueve nunca. */
  fecha_nominal: string;
  /** El día contra el que se mide el cumplimiento. */
  fecha_limite: string;
  tipo_ajuste: 'NINGUNO' | 'ANTICIPADO' | 'GRACIA' | 'OMITIR';
  estado: 'PENDIENTE' | 'CUMPLIDO' | 'EXTEMPORANEO' | 'INCUMPLIDO';
  archivo_nombre: string | null;
  tiene_firma: boolean;
  /** Factor de calificación por contenido (#165): `false` = el contenido no
   *  concuerda con la extensión (marcado para revisión); `true` = concuerda;
   *  `null` = sin archivo o tipo no determinable. */
  archivo_valido: boolean | null;
  /** Estado de la justificación de un incumplimiento (#163): null sin justificar,
   *  'PENDIENTE' esperando revisión, 'ACEPTADA' / 'RECHAZADA'. */
  justificacion_estado: 'PENDIENTE' | 'ACEPTADA' | 'RECHAZADA' | null;
}

/** Una notificación interna del flujo del supervisor (#163). */
export interface Notificacion {
  id: number;
  tipo: string;
  titulo: string;
  mensaje: string;
  entrega_id: number | null;
  leida: boolean;
  momento: string;
}

export interface FilaPrevia {
  nominal: string;
  limite: string;
  ajuste: string;
  se_movio: boolean;
}

/** El formato que el servidor ya entiende, el mismo que hay guardado.
 *
 *  `INMEDIATO` es «a solicitud»: el informe existe pero no vence por calendario.
 *  No lo produce el constructor de reglas —no hay nada que construir—, pero sí
 *  aparece en informes migrados, así que el tipo tiene que admitirlo. */
export interface ReglaJson {
  modo: 'CICLICO' | 'ESPECIFICO' | 'SEMANAL' | 'HOURLY' | 'FECHAS' | 'INMEDIATO';
  fecha_inicio?: string;
  intervalo?: number;
  meses?: number[];
  dia_corte?: number;
  dias_corte?: number[];
  es_ultimo_dia?: boolean;
  dia_habil?: number;
  dias_semana?: number[];
  dia_habil_semana?: number;
  fechas?: string[];
  mover_habil?: boolean;
  mover_habil_anterior?: boolean;
  omitir_festivos?: boolean;
}

/** Una lectura posible de los días marcados en el calendario.
 *
 *  La produce el motor, no la pantalla: `dice` es la frase en los términos de
 *  quien crea la obligación y `proximas` son las fechas que esa regla generaría
 *  de verdad. Elegir una lectura es elegir su `regla`, así que lo que se guarda
 *  es exactamente lo que se mostró. */
export interface Lectura {
  periodicidad: string | null;
  nombre: string;
  dice: string;
  regla: ReglaJson;
  proximas: string[];
}

/** Cómo tratar los días no hábiles. El motor NO lo deduce de las fechas
 *  marcadas —de unas fechas no se sigue si la entidad da plazo o exige
 *  anticipar—, así que lo elige el usuario y viaja con la selección para que las
 *  fechas mostradas sean las que se van a guardar. */
export interface AjusteSel {
  gracia: boolean;
  anticipado: boolean;
  omitir_festivos: boolean;
}

/** Un informe en la lista de gestión. `modo` sale ya extraído de la regla. */
export interface InformeFila {
  id: number;
  nombre: string;
  descripcion: string | null;
  activo: boolean;
  modo: string | null;
}

/** El detalle, con la regla cruda para cargarla en el editor. */
export interface InformeDetalle {
  id: number;
  nombre: string;
  descripcion: string | null;
  activo: boolean;
  /** El JSON de la regla, tal como está guardado. */
  recurrencia: string;
}

/** Lo que el formulario manda al crear o editar un informe. */
export interface InformeEntrada {
  nombre: string;
  descripcion?: string | null;
  recurrencia: ReglaJson;
  activo?: boolean;
}

/** Los papeles de una parte en un informe. */
export type RolParte = 'ORIGEN' | 'DESTINO' | 'USUARIO' | 'SUPERVISA' | 'EJECUTA';

/** Una parte de un informe, con el nombre ya resuelto. */
export interface Parte {
  id: number;
  usuario_id: number | null;
  stakeholder_id: number | null;
  nombre: string;
  rol: RolParte;
  es_principal: boolean;
  orden: number;
}

/** Lo que el editor manda al asignar partes. */
export interface ParteEntrada {
  usuario_id?: number | null;
  stakeholder_id?: number | null;
  rol: RolParte;
  es_principal?: boolean;
  orden?: number;
}

/** Un usuario o stakeholder, para los selectores del editor de partes. */
export interface OpcionParte {
  id: number;
  nombre: string;
  detalle: string | null;
}

@Injectable({ providedIn: 'root' })
export class Api {
  private http = inject(HttpClient);

  /** Mensaje legible. El servidor nunca manda detalles internos, así que lo que
   *  llega ya está pensado para leerse. */
  private falla(e: HttpErrorResponse) {
    const mensaje =
      e.status === 0
        ? 'No hay conexión con el servidor.'
        : (e.error?.error ?? `Error ${e.status}`);
    return throwError(() => new Error(mensaje));
  }

  private get<T>(url: string): Observable<T> {
    return this.http
      .get<T>(url, { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  entrar(email: string, clave: string) {
    return this.http
      .post<{ ok: boolean }>('/api/sesion', { email, clave }, { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  salir() {
    return this.http
      .delete<{ ok: boolean }>('/api/sesion', { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  yo() {
    return this.get<Actor>('/api/yo');
  }

  entregas() {
    return this.get<Entrega[]>('/api/entregas');
  }

  /**
   * La URL de descarga lleva el **id**, nunca una ruta.
   *
   * En el sistema anterior el navegador mandaba la ruta del archivo y el
   * servidor la abría: un tercero externo podía pedir el `.env`. Aquí la ruta la
   * construye el servidor a partir del id, y solo después de comprobar que esa
   * entrega está dentro de lo que este actor puede ver.
   */
  urlArchivo(entregaId: number) {
    return `/api/entregas/${entregaId}/archivo`;
  }

  /**
   * Registrar la entrega subiendo su archivo.
   *
   * Del nombre del archivo el servidor toma **solo la extensión**; el nombre con
   * el que se guarda lo construye él desde el id. Por eso aquí no hay ningún
   * campo de ruta ni de carpeta: no existe nada que el navegador pueda decir
   * sobre dónde acaba el archivo.
   */
  subirArchivo(entregaId: number, archivo: File) {
    const cuerpo = new FormData();
    cuerpo.append('archivo', archivo, archivo.name);
    return this.http
      .post<{ ok: boolean; estado: string; archivo: string; bytes: number }>(
        this.urlArchivo(entregaId),
        cuerpo,
        { withCredentials: true },
      )
      .pipe(catchError((e) => this.falla(e)));
  }

  previsualizar(regla: ReglaJson, desde: string, hasta: string) {
    return this.http
      .post<FilaPrevia[]>(
        '/api/periodicidad/previsualizar',
        { regla, desde, hasta },
        { withCredentials: true },
      )
      .pipe(catchError((e) => this.falla(e)));
  }

  // --- Gestión de informes y sus reglas -------------------------------------

  informes() {
    return this.get<InformeFila[]>('/api/informes');
  }

  informeDetalle(id: number) {
    return this.get<InformeDetalle>(`/api/informes/${id}`);
  }

  /** Qué puede ser lo que se marcó en el calendario. */
  lecturas(fechas: string[], ajuste: AjusteSel) {
    return this.http
      .post<{ lecturas: Lectura[] }>(
        '/api/cronograma/lecturas',
        { fechas, ajuste },
        { withCredentials: true },
      )
      .pipe(catchError((e) => this.falla(e)));
  }

  crearInforme(cuerpo: InformeEntrada) {
    return this.http
      .post<{ id: number }>('/api/informes', cuerpo, { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  actualizarInforme(id: number, cuerpo: InformeEntrada) {
    return this.http
      .put<{ ok: boolean }>(`/api/informes/${id}`, cuerpo, { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  // --- Participación (quién interviene en un informe) -----------------------

  participacion(informeId: number) {
    return this.get<Parte[]>(`/api/informes/${informeId}/participacion`);
  }

  guardarParticipacion(informeId: number, partes: ParteEntrada[]) {
    return this.http
      .put<{ ok: boolean; partes: number }>(`/api/informes/${informeId}/participacion`, partes, {
        withCredentials: true,
      })
      .pipe(catchError((e) => this.falla(e)));
  }

  usuarios() {
    return this.get<OpcionParte[]>('/api/usuarios');
  }

  stakeholders() {
    return this.get<OpcionParte[]>('/api/stakeholders');
  }

  estadisticas() {
    return this.get<Estadisticas>('/api/estadisticas');
  }

  // --- Flujo del supervisor (#163) ------------------------------------------

  notificaciones() {
    return this.get<Notificacion[]>('/api/notificaciones');
  }

  marcarLeida(id: number) {
    return this.http
      .post<{ ok: boolean }>(`/api/notificaciones/${id}/leida`, {}, { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  justificar(entregaId: number, texto: string) {
    return this.http
      .post<{ ok: boolean; estado: string }>(`/api/entregas/${entregaId}/justificacion`, { texto }, { withCredentials: true })
      .pipe(catchError((e) => this.falla(e)));
  }

  revisar(entregaId: number, aceptar: boolean, retroalimentacion: string) {
    return this.http
      .post<{ ok: boolean; estado: string }>(
        `/api/entregas/${entregaId}/revision`,
        { aceptar, retroalimentacion },
        { withCredentials: true },
      )
      .pipe(catchError((e) => this.falla(e)));
  }
}

/** Resumen de cumplimiento, ya recortado por el alcance del actor en el servidor. */
export interface Estadisticas {
  total: number;
  pendientes: number;
  cumplidas: number;
  extemporaneas: number;
  incumplidas: number;
  proximas_7d: number;
  vencidas: number;
  /** Cumplidas sobre lo ya vencido, 0..100; `null` si nada ha vencido aún. */
  tasa_cumplimiento: number | null;
}
