/**
 * El segundo factor de la cuenta propia (#212): estado, enrolamiento y baja.
 *
 * Tres momentos en una sola pantalla porque son tres estados del mismo hecho —la
 * cuenta tiene o no tiene segundo factor—, y repartirlos en tres rutas obligaría
 * a navegar en medio del enrolamiento, que es justo cuando el QR desaparece.
 *
 * # El QR llega dibujado
 *
 * El servidor manda el SVG hecho (`qr.rs`) y aquí solo se pinta, dentro de un
 * `<img>` con `data:`. No hay librería de QR en `package.json`, y no la va a
 * haber: son veinte kilobytes en el paquete inicial para una pantalla que cada
 * cuenta abre una vez. Meterlo en un `<img>` y no con `innerHTML` también aísla:
 * el navegador trata ese SVG como un documento aparte y sin guiones.
 *
 * # Lo que esta pantalla no puede volver a enseñar
 *
 * Los códigos de recuperación se entregan **una sola vez**: en la base solo queda
 * su hash. Por eso el paso final no se puede cerrar por descuido —hay que
 * confirmar que se guardaron— y por eso el aviso está antes de los códigos y no
 * después.
 */
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { firstValueFrom } from 'rxjs';

import { Api, Enrolamiento, EstadoFactor } from '../nucleo/api';

@Component({
  selector: 'siger-segundo-factor',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
  ],
  template: `
    <mat-card class="tarjeta">
      <h2>Verificación en dos pasos</h2>

      @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
      @if (error()) {
        <p class="error" role="alert"><mat-icon>error_outline</mat-icon> {{ error() }}</p>
      }
      @if (hecho()) {
        <p class="bien" role="status"><mat-icon>check_circle</mat-icon> {{ hecho() }}</p>
      }

      <!-- 1. Los códigos recién generados. Manda sobre todo lo demás: mientras
           estén en pantalla no hay otra cosa que hacer que guardarlos. -->
      @if (codigos(); as lista) {
        <section class="recuperacion">
          <p class="aviso">
            <mat-icon>warning</mat-icon>
            <span>
              Guarde estos códigos <strong>ahora</strong>. No se pueden volver a
              consultar: el servidor solo conserva su huella. Cada uno sirve una
              vez, y entran en lugar del código del teléfono si lo pierde.
            </span>
          </p>
          <ul class="codigos">
            @for (c of lista; track c) { <li>{{ c }}</li> }
          </ul>
          <div class="acciones">
            <button mat-button type="button" (click)="copiar(lista)">
              <mat-icon>content_copy</mat-icon> Copiar
            </button>
            <button mat-flat-button type="button" (click)="cerrarCodigos()">
              Ya los guardé
            </button>
          </div>
        </section>
      } @else if (enrolamiento(); as e) {
        <!-- 2. Enrolamiento en curso: el QR y la casilla que lo confirma. -->
        <section class="enrolando">
          <ol class="pasos">
            <li>Abra su aplicación de autenticación y escanee este código.</li>
            <li>Escriba abajo el número de seis dígitos que muestre.</li>
          </ol>

          <img class="qr" [src]="qrComoImagen(e)"
               alt="Código QR para enrolar la cuenta en la aplicación de autenticación" />

          <p class="a-mano">
            ¿La cámara no lo lee? Escriba esta clave a mano:
            <code>{{ e.secreto }}</code>
          </p>

          <mat-form-field>
            <mat-label>Código de seis dígitos</mat-label>
            <input matInput name="codigo" inputmode="numeric" autocomplete="one-time-code"
                   [(ngModel)]="codigo" />
            <mat-hint>Si no coincide, revise la hora del teléfono.</mat-hint>
          </mat-form-field>

          <div class="acciones">
            <button mat-button type="button" (click)="cancelarEnrolamiento()">Cancelar</button>
            <button mat-flat-button type="button" [disabled]="cargando()" (click)="activar()">
              Activar
            </button>
          </div>
        </section>
      } @else if (estado(); as s) {
        <!-- 3. El estado de siempre: puesto o no puesto. -->
        @if (s.activo) {
          <p class="estado activo">
            <mat-icon>verified_user</mat-icon>
            La verificación en dos pasos está <strong>activa</strong>.
          </p>
          <p class="guia">
            Le quedan {{ s.codigos_disponibles }} códigos de recuperación sin usar.
            @if (pocosCodigos()) {
              <strong>
                Quedan pocos: desactive y vuelva a activar para generar diez
                nuevos.
              </strong>
            }
          </p>

          <h3>Desactivar</h3>
          <p class="guia">
            Hace falta su contraseña: sin ella, cualquiera con la sesión abierta
            podría apagarla. Se borrarán también los códigos de recuperación.
          </p>
          <mat-form-field>
            <mat-label>Contraseña actual</mat-label>
            <input matInput type="password" autocomplete="current-password"
                   [(ngModel)]="clave" />
          </mat-form-field>
          <div class="acciones">
            <button mat-button (click)="volver()">Volver</button>
            <button mat-stroked-button [disabled]="cargando()" (click)="desactivar()">
              Desactivar
            </button>
          </div>
        } @else {
          <p class="estado">
            <mat-icon>gpp_maybe</mat-icon>
            La verificación en dos pasos <strong>no está activa</strong>.
          </p>
          <p class="guia">
            Con ella puesta, saber su contraseña ya no basta para entrar: hace
            falta además el código que cambia cada 30 segundos en su teléfono.
          </p>

          @if (!s.puede_activarse) {
            <!-- Se dice el motivo real. Un botón que falla al pulsarlo hace que
                 la persona lo intente tres veces y llame por teléfono. -->
            <p class="error" role="alert">
              <mat-icon>build</mat-icon>
              Esta instalación no tiene configurada la frase de cifrado en reposo
              (<code>SIGER_CLAVE_REPOSO</code>), y sin ella el secreto se
              guardaría en claro. Avise a quien administre el servidor.
            </p>
          }

          <div class="acciones">
            <button mat-button (click)="volver()">Volver</button>
            <button mat-flat-button [disabled]="cargando() || !s.puede_activarse"
                    (click)="iniciar()">
              Activar
            </button>
          </div>
        }
      }
    </mat-card>
  `,
  styles: [
    `
      .tarjeta { max-width: 32rem; margin: 1rem auto; padding: 1.5rem; display: flex; flex-direction: column; gap: 0.75rem; }
      h2, h3 { margin: 0; }
      h3 { margin-top: 1rem; }
      .guia { color: var(--mat-sys-on-surface-variant); font-size: 0.9rem; margin: 0; }
      .error, .bien, .estado { display: flex; align-items: center; gap: 0.4rem; margin: 0; font-size: 0.95rem; }
      .error { color: var(--mat-sys-error); align-items: flex-start; }
      .bien, .activo { color: var(--mat-sys-primary); }
      .acciones { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 0.5rem; }
      .enrolando, .recuperacion { display: flex; flex-direction: column; gap: 0.75rem; }
      .pasos { margin: 0; padding-left: 1.2rem; color: var(--mat-sys-on-surface-variant); font-size: 0.9rem; }
      /* El QR se pinta sobre blanco pase lo que pase con el tema: el fondo va
         dentro del propio SVG, y este relleno es solo la zona de respeto. */
      .qr { width: min(15rem, 100%); align-self: center; background: #fff; border-radius: 4px; }
      .a-mano { margin: 0; font-size: 0.9rem; }
      .a-mano code, .error code { font-family: monospace; word-break: break-all; }
      .aviso { display: flex; gap: 0.5rem; align-items: flex-start; margin: 0; font-size: 0.9rem; }
      /* El flex-shrink no es adorno: sin él la fila estruja el icono hasta dejar
         una raya roja de dos píxeles, y el aviso que dice «esto no se puede
         volver a ver» pierde justo la parte que se mira antes de leer. */
      .aviso mat-icon { color: var(--mat-sys-error); flex-shrink: 0; }
      .codigos {
        list-style: none; margin: 0; padding: 0.75rem;
        display: grid; grid-template-columns: repeat(2, 1fr); gap: 0.35rem;
        font-family: monospace; font-size: 1rem;
        background: var(--mat-sys-surface-container-high); border-radius: 4px;
      }
    `,
  ],
})
export class SegundoFactor {
  private api = inject(Api);
  private router = inject(Router);

  codigo = '';
  clave = '';

  readonly cargando = signal(false);
  readonly error = signal('');
  readonly hecho = signal('');
  readonly estado = signal<EstadoFactor | null>(null);
  readonly enrolamiento = signal<Enrolamiento | null>(null);
  readonly codigos = signal<string[] | null>(null);

  /** Tres o menos. No es una cifra mágica: es cuando conviene avisar antes de
   *  que alguien se quede sin salida con el teléfono perdido. */
  readonly pocosCodigos = computed(() => (this.estado()?.codigos_disponibles ?? 0) <= 3);

  constructor() {
    void this.recargar();
  }

  /**
   * El SVG que manda el servidor, como imagen.
   *
   * `encodeURIComponent` y no base64: el SVG es texto ASCII, así queda legible en
   * el inspector y no hace falta `btoa`. La CSP del binario admite `img-src
   * 'self' data:`, que es exactamente esto y nada más.
   */
  qrComoImagen(e: Enrolamiento) {
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(e.qr_svg)}`;
  }

  private async recargar() {
    this.cargando.set(true);
    try {
      this.estado.set(await firstValueFrom(this.api.estadoSegundoFactor()));
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  async iniciar() {
    this.error.set('');
    this.hecho.set('');
    this.cargando.set(true);
    try {
      this.enrolamiento.set(await firstValueFrom(this.api.iniciarSegundoFactor()));
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  async activar() {
    this.error.set('');
    this.cargando.set(true);
    try {
      this.codigos.set(await firstValueFrom(this.api.activarSegundoFactor(this.codigo.trim())));
      this.codigo = '';
      this.enrolamiento.set(null);
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  /**
   * Salir del enrolamiento sin activarlo.
   *
   * No se llama a nada: el secreto queda guardado sin confirmar y el próximo
   * «Activar» lo reemplaza. Mientras `totp_confirmado_en` sea nulo, ese secreto
   * no abre ninguna puerta.
   */
  cancelarEnrolamiento() {
    this.enrolamiento.set(null);
    this.codigo = '';
    this.error.set('');
  }

  async cerrarCodigos() {
    this.codigos.set(null);
    this.hecho.set('Verificación en dos pasos activada.');
    await this.recargar();
  }

  async desactivar() {
    this.error.set('');
    this.hecho.set('');
    this.cargando.set(true);
    try {
      await firstValueFrom(this.api.desactivarSegundoFactor(this.clave));
      this.clave = '';
      this.hecho.set('Verificación en dos pasos desactivada.');
      await this.recargar();
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  /**
   * Copiar al portapapeles.
   *
   * Si el navegador no deja (sin HTTPS, o sin permiso), se dice: quedarse callado
   * haría creer que los códigos están copiados cuando no lo están, y esta es la
   * única vez que se pueden ver.
   */
  async copiar(lista: string[]) {
    try {
      await navigator.clipboard.writeText(lista.join('\n'));
      this.hecho.set('Códigos copiados al portapapeles.');
    } catch {
      this.error.set('El navegador no dejó copiar. Anótelos a mano antes de continuar.');
    }
  }

  volver() {
    void this.router.navigate(['/entregas']);
  }
}
