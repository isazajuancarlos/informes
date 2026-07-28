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
}

export interface FilaPrevia {
  nominal: string;
  limite: string;
  ajuste: string;
  se_movio: boolean;
}

/** El formato que el servidor ya entiende, el mismo que hay guardado. */
export interface ReglaJson {
  modo: 'CICLICO' | 'ESPECIFICO' | 'SEMANAL' | 'HOURLY' | 'FECHAS';
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

  previsualizar(regla: ReglaJson, desde: string, hasta: string) {
    return this.http
      .post<FilaPrevia[]>(
        '/api/periodicidad/previsualizar',
        { regla, desde, hasta },
        { withCredentials: true },
      )
      .pipe(catchError((e) => this.falla(e)));
  }
}
